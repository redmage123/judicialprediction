//! S6.5 — Citation-graph populator over `case_documents`.
//!
//! Reads `case_documents.cites_json` (raw CourtListener `opinions_cited` URI
//! arrays captured at REST-ingest time) and inserts `(citing, cited)` edges
//! into `case_document_citations` — but **only** for cited opinions that
//! already exist locally.  Dangling pointers to opinions outside the corpus
//! are dropped silently; the next ingest day may bring them in, at which
//! point a re-run will pick up the now-resolvable edges.
//!
//! Idempotent: edges are written via `ON CONFLICT DO NOTHING`.

use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::HashSet;

/// Per-run stats returned by [`populate_citations`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CitationStats {
    /// Rows in `case_documents` we scanned that already had cites_json.
    pub citing_docs_scanned: u64,
    /// URIs we tried to parse out of cites_json.
    pub cites_seen: u64,
    /// URIs that didn't match the `/opinions/<id>/` shape — usually a
    /// link to a docket or cluster rather than an opinion.
    pub cites_unparseable: u64,
    /// URIs that parsed cleanly but pointed at an opinion not in our
    /// corpus.  These are not errors — see the module docstring.
    pub cites_dangling: u64,
    /// Edges inserted (new).
    pub edges_inserted: u64,
    /// Edges that already existed (re-run, no change).
    pub edges_existing: u64,
}

/// Walk `case_documents` and populate `case_document_citations`.
pub async fn populate_citations(pool: &PgPool) -> Result<CitationStats> {
    let mut stats = CitationStats::default();

    // 1. Load the local universe of opinion_ids so we can drop dangling edges
    //    in a single hash lookup rather than per-row FK probes.
    let known_ids: HashSet<i64> = sqlx::query_scalar(
        "SELECT opinion_id FROM case_documents",
    )
    .fetch_all(pool)
    .await
    .context("load opinion_id universe")?
    .into_iter()
    .collect();

    // 2. Stream the citing rows that have cites_json populated.
    let citing_rows: Vec<(i64, serde_json::Value)> = sqlx::query_as(
        "SELECT opinion_id, cites_json FROM case_documents WHERE cites_json IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .context("load cites_json rows")?;

    for (citing_id, cites_value) in &citing_rows {
        stats.citing_docs_scanned += 1;
        for uri in extract_uri_strings(cites_value) {
            stats.cites_seen += 1;
            let Some(cited_id) = parse_opinion_id_from_uri(&uri) else {
                stats.cites_unparseable += 1;
                continue;
            };
            if cited_id == *citing_id {
                // Self-cite — schema CHECK would reject anyway; bookkeep as
                // unparseable to keep callers honest.
                stats.cites_unparseable += 1;
                continue;
            }
            if !known_ids.contains(&cited_id) {
                stats.cites_dangling += 1;
                continue;
            }
            let inserted = sqlx::query(
                r#"
                INSERT INTO case_document_citations
                    (citing_opinion_id, cited_opinion_id)
                VALUES ($1, $2)
                ON CONFLICT (citing_opinion_id, cited_opinion_id) DO NOTHING
                "#,
            )
            .bind(citing_id)
            .bind(cited_id)
            .execute(pool)
            .await
            .with_context(|| {
                format!("insert citation {citing_id} -> {cited_id}")
            })?;
            if inserted.rows_affected() == 1 {
                stats.edges_inserted += 1;
            } else {
                stats.edges_existing += 1;
            }
        }
    }

    Ok(stats)
}

/// Pure helper: pull URI strings out of a `cites_json` value.
///
/// Returns an empty Vec for any shape that isn't an array of strings (the
/// expected shape from CourtListener).  Non-string array elements are
/// silently skipped.
pub fn extract_uri_strings(v: &serde_json::Value) -> Vec<String> {
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}

/// Pure helper: parse the trailing `<id>` from a CourtListener opinions URI.
///
/// Accepts both absolute (`https://www.courtlistener.com/api/rest/v4/opinions/12345/`)
/// and root-relative (`/api/rest/v4/opinions/12345/`) forms, with or without
/// a trailing slash.  Returns `None` for anything that doesn't match the
/// `/opinions/<digits>/?` pattern.
pub fn parse_opinion_id_from_uri(uri: &str) -> Option<i64> {
    let marker = "/opinions/";
    let after = uri.split(marker).nth(1)?;
    // Walk forward while we see ASCII digits; stop at the first non-digit
    // (either `/`, `?`, or end-of-string).  Cheaper than regex, and the
    // shape is fixed enough that anything else is a hostile URI.
    let id_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if id_str.is_empty() {
        return None;
    }
    id_str.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_id_from_absolute_uri() {
        assert_eq!(
            parse_opinion_id_from_uri(
                "https://www.courtlistener.com/api/rest/v4/opinions/12345/"
            ),
            Some(12345)
        );
    }

    #[test]
    fn parse_id_from_root_relative_uri() {
        assert_eq!(
            parse_opinion_id_from_uri("/api/rest/v4/opinions/42/"),
            Some(42)
        );
    }

    #[test]
    fn parse_id_handles_no_trailing_slash() {
        assert_eq!(parse_opinion_id_from_uri("/opinions/777"), Some(777));
    }

    #[test]
    fn parse_id_returns_none_for_non_opinion_uri() {
        // Cluster URIs share the URL family but aren't opinions.
        assert_eq!(
            parse_opinion_id_from_uri("https://www.courtlistener.com/api/rest/v4/clusters/12345/"),
            None
        );
        assert_eq!(parse_opinion_id_from_uri(""), None);
        assert_eq!(parse_opinion_id_from_uri("not a uri"), None);
    }

    #[test]
    fn parse_id_returns_none_for_missing_id() {
        // /opinions/ followed by non-digits is a malformed URI in this
        // context — better to skip than to silently match.
        assert_eq!(parse_opinion_id_from_uri("/opinions//"), None);
        assert_eq!(parse_opinion_id_from_uri("/opinions/abc/"), None);
    }

    #[test]
    fn extract_uris_from_array_value() {
        let v = json!([
            "https://www.courtlistener.com/api/rest/v4/opinions/1/",
            "/api/rest/v4/opinions/2/",
            42,           // non-string -> skipped
            null,         // non-string -> skipped
        ]);
        let uris = extract_uri_strings(&v);
        assert_eq!(uris.len(), 2);
        assert!(uris[0].ends_with("/opinions/1/"));
        assert!(uris[1].ends_with("/opinions/2/"));
    }

    #[test]
    fn extract_uris_returns_empty_for_non_array() {
        assert!(extract_uri_strings(&json!(null)).is_empty());
        assert!(extract_uri_strings(&json!({"foo": "bar"})).is_empty());
        assert!(extract_uri_strings(&json!("string")).is_empty());
    }
}
