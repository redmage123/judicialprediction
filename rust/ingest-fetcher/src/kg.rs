//! Knowledge-graph populator (S5.6).
//!
//! Walks `case_documents` and writes nodes to the S5.5 KG tables (`courts`,
//! `judges`).  Idempotent: re-runs upsert via `ON CONFLICT DO NOTHING` so the
//! cron / dev-loop replay stays clean.
//!
//! # Out of scope (deferred to later sprints)
//!
//! - **`case_courts` / `case_judges` / `case_citations` edges.**  These FK
//!   to `cases(id)`, but the public CourtListener corpus lives in a separate
//!   table (`case_documents`); we have no operator-created `cases` rows to
//!   attach the edges to.  Future work: either import case_documents into
//!   `cases` with a `case_origin = 'public_corpus'` discriminator, or extend
//!   the edge tables to accept either source.
//!
//! - **Citations from the CourtListener `cites` array.**  The `case_documents`
//!   schema (S2.17) does not store the `cites` array; the ingest fetcher
//!   would need a column add + a re-pull.  Tracked as Sprint 6 follow-up.
//!
//! # What it does ship
//!
//! 1. Court nodes — one row per `DISTINCT case_documents.court_id`, keyed on
//!    `(tenant_id, source='courtlistener', source_id=court_id)`.
//! 2. Judge nodes — regex-extracted from the opinion header (first ~5 KB of
//!    `full_text_plain`), unique on `(tenant_id, normalized_name)`, linked
//!    to the court they were first seen in via `primary_court_id`.

use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Per-run stats returned by [`populate_from_case_documents`].
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PopulateStats {
    pub case_documents_scanned: u64,
    pub courts_inserted: u64,
    pub courts_existing: u64,
    pub judges_inserted: u64,
    pub judges_existing: u64,
}

/// Walk `case_documents` and populate `courts` + `judges` for `tenant_id`.
///
/// Connection must run with sufficient privileges to write the KG tables
/// (either the migration role or `jp_app` with `SET LOCAL app.current_tenant_id`).
/// This function does the `SET LOCAL` itself so RLS sees the tenant.
pub async fn populate_from_case_documents(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<PopulateStats> {
    let mut tx = pool.begin().await.context("begin populate-kg tx")?;

    // RLS reads `app.current_tenant_id`; set it for this transaction so the
    // KG inserts pass the `tenant_isolation` USING/WITH CHECK clauses.
    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{tenant_id}'"
    ))
    .execute(&mut *tx)
    .await
    .context("SET LOCAL app.current_tenant_id")?;

    let mut stats = PopulateStats::default();

    // ── 1. Courts ──────────────────────────────────────────────────────────
    // Read distinct CL court slugs and the jurisdiction they map to.  We only
    // store one jurisdiction string per court — CL's `tax` slug is treated
    // as `us-federal` for now (other slugs map to `us-state` or `us-federal`
    // via map_courtlistener_jurisdiction).
    let court_rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT court_id FROM case_documents ORDER BY court_id",
    )
    .fetch_all(&mut *tx)
    .await
    .context("select distinct court_id")?;

    // court_id (CL slug) -> internal courts.id
    let mut court_ids: BTreeMap<String, Uuid> = BTreeMap::new();
    for (slug,) in &court_rows {
        let jurisdiction = map_courtlistener_jurisdiction(slug);
        let name = canonical_court_name(slug);

        // Upsert via DO UPDATE on the unique key so RETURNING always yields
        // the row id regardless of whether this run inserted or matched.
        // `excluded.source_id = excluded.source_id` keeps the row's data
        // intact on conflict (effectively a no-op write) while still
        // satisfying ON CONFLICT's syntax requirement.
        let inserted_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO courts (tenant_id, name, jurisdiction, source, source_id)
            VALUES ($1, $2, $3, 'courtlistener', $4)
            ON CONFLICT (tenant_id, name) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(&name)
        .bind(jurisdiction)
        .bind(slug)
        .fetch_optional(&mut *tx)
        .await
        .with_context(|| format!("insert court {name}"))?;

        let (id, was_inserted) = match inserted_id {
            Some(id) => (id, true),
            None => {
                let id: Uuid = sqlx::query_scalar(
                    "SELECT id FROM courts WHERE tenant_id = $1 AND name = $2",
                )
                .bind(tenant_id)
                .bind(&name)
                .fetch_one(&mut *tx)
                .await
                .with_context(|| format!("select existing court {name}"))?;
                (id, false)
            }
        };

        court_ids.insert(slug.clone(), id);
        if was_inserted {
            stats.courts_inserted += 1;
        } else {
            stats.courts_existing += 1;
        }
    }

    // ── 2. Judges ──────────────────────────────────────────────────────────
    // Stream opinion headers and accumulate distinct judges keyed by
    // normalized name.  First-court-seen wins for `primary_court_id`.
    let docs: Vec<(String, String)> = sqlx::query_as(
        "SELECT court_id, full_text_plain FROM case_documents",
    )
    .fetch_all(&mut *tx)
    .await
    .context("select case_documents")?;

    stats.case_documents_scanned = docs.len() as u64;

    // normalized_name -> (full_name, primary_court_slug)
    let mut judges_seen: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (court_slug, full_text) in &docs {
        // Look at the opinion header (first ~5 KB) where judge names appear.
        let head = head_chars(full_text, 5_000);
        for full_name in extract_judge_names(&head) {
            let norm = normalize_judge_name(&full_name);
            if norm.is_empty() {
                continue;
            }
            judges_seen
                .entry(norm)
                .or_insert_with(|| (full_name, court_slug.clone()));
        }
    }

    for (normalized, (full_name, court_slug)) in &judges_seen {
        let primary_court_id = court_ids.get(court_slug).copied();

        let inserted_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO judges (
                tenant_id, full_name, normalized_name, primary_court_id, source
            )
            VALUES ($1, $2, $3, $4, 'courtlistener')
            ON CONFLICT (tenant_id, normalized_name) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(full_name)
        .bind(normalized)
        .bind(primary_court_id)
        .fetch_optional(&mut *tx)
        .await
        .with_context(|| format!("insert judge {full_name}"))?;

        if inserted_id.is_some() {
            stats.judges_inserted += 1;
        } else {
            stats.judges_existing += 1;
        }
    }

    tx.commit().await.context("commit populate-kg tx")?;
    Ok(stats)
}

/// Map a CourtListener court slug to a coarse jurisdiction string.
///
/// First-pass: the only slug currently ingested is `tax`.  Future slugs are
/// classified via prefix conventions; unrecognised slugs default to `unknown`
/// rather than guessing.
pub(crate) fn map_courtlistener_jurisdiction(slug: &str) -> &'static str {
    match slug {
        // US Tax Court is a federal Article-I court.
        "tax" => "us-federal",
        // CourtListener federal slugs: scotus, ca1..ca11, cafc, cadc, dcd, ...
        s if s == "scotus" => "us-federal",
        s if s.starts_with("ca") => "us-federal",
        s if s.ends_with("d") && s.len() == 4 => "us-federal", // e.g. nyed, casd
        // CL state slugs are typically 2-letter prefixes (cal, ny, tx, ...).
        s if s.len() == 2 || s.len() == 3 => "us-state",
        _ => "unknown",
    }
}

/// Human-readable court name for a CL slug.
pub(crate) fn canonical_court_name(slug: &str) -> String {
    match slug {
        "tax" => "United States Tax Court".to_string(),
        "scotus" => "Supreme Court of the United States".to_string(),
        // Fallback: prefix with "Court (" + slug + ")" so the row is
        // distinguishable even before S5.7 NLP catches up.
        _ => format!("Court ({slug})"),
    }
}

/// Truncate `s` to the first `max_chars` Unicode characters.
fn head_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// Extract candidate judge names from an opinion header.
///
/// Looks for two common Tax-Court patterns:
///   1. `NAME, Judge:`              (line-leading proper-name list)
///   2. `JUDGE NAME.` / `NAME J.`   (caps-only signatures)
///
/// Returns each unique candidate in source order; the caller normalizes.
pub(crate) fn extract_judge_names(text: &str) -> Vec<String> {
    let mut hits = BTreeSet::new();
    let mut ordered = Vec::new();
    for line in text.lines() {
        for cand in extract_judges_from_line(line) {
            if hits.insert(cand.clone()) {
                ordered.push(cand);
            }
        }
    }
    ordered
}

/// Line-level extractor — small, dependency-free, tax-court aware.
///
/// Recognised shapes (case-sensitive):
///   * `NAME, Judge:`                 → captures NAME
///   * `NAME, Judge.`                 → captures NAME
///   * `NAME, J., delivered`          → captures NAME
///   * `JUDGE NAME delivered`         → captures NAME (caps-only)
///
/// Anything else returns empty.  Names are accepted as 1–3 whitespace-
/// separated word-tokens of letters/hyphens/apostrophes — broader patterns
/// would need a real grammar.
fn extract_judges_from_line(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.len() > 200 {
        return Vec::new();
    }
    let mut out = Vec::new();

    // Pattern 1 / 2 / 3: `<name>, Judge:` / `<name>, Judge.` / `<name>, J., delivered`
    // Break after the first marker hit so a line like
    // `Holmes, J., delivered the opinion` (which contains both `, J., delivered`
    // and `, J.,` as substrings) yields one candidate, not two.
    for marker in [", Judge:", ", Judge.", ", J., delivered", ", J.,"] {
        if let Some(idx) = trimmed.find(marker) {
            let candidate = trimmed[..idx].trim();
            if is_proper_name_candidate(candidate) {
                out.push(candidate.to_string());
                break;
            }
        }
    }

    // Pattern 4: `JUDGE <NAME> delivered ...`
    if let Some(rest) = trimmed.strip_prefix("JUDGE ") {
        let name = rest
            .split_whitespace()
            .take_while(|w| w.chars().all(|c| c.is_uppercase() || c == '.' || c == ','))
            .collect::<Vec<_>>()
            .join(" ");
        let name = name.trim_end_matches(',').trim_end_matches('.');
        if !name.is_empty() && is_proper_name_candidate(name) {
            out.push(name.to_string());
        }
    }

    out
}

/// Return true if `s` looks like a tax-court judge name in opinion-preamble
/// shape.  The corpus uses two narrow conventions:
///   * single capitalized token (`Holmes`, `GOEKE`)
///   * all-uppercase multi-token strings (`PATRICIA A. TORRES`)
///
/// Mixed-case multi-token strings like `Some Random Words` are rejected
/// because they don't match either real-world shape and tend to be
/// false-positives off non-preamble lines.
fn is_proper_name_candidate(s: &str) -> bool {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.is_empty() || tokens.len() > 3 {
        return false;
    }
    // All chars must be letters / `-` / `'` / `.`
    for t in &tokens {
        for c in t.chars() {
            if !(c.is_alphabetic() || c == '-' || c == '\'' || c == '.') {
                return false;
            }
        }
    }
    // Single token: just needs an uppercase letter.
    if tokens.len() == 1 {
        return tokens[0].chars().any(|c| c.is_uppercase());
    }
    // Multi-token: every token must be all-uppercase (titles/initials count).
    // This rejects mixed-case false-positives like "Some Random Words".
    tokens.iter().all(|t| {
        let letters: String = t.chars().filter(|c| c.is_alphabetic()).collect();
        !letters.is_empty() && letters.chars().all(|c| c.is_uppercase())
    })
}

/// Lowercase, strip titles, collapse whitespace.  This is the canonical
/// match key for `judges.normalized_name`.
pub(crate) fn normalize_judge_name(raw: &str) -> String {
    let s = raw.to_lowercase();
    // Strip leading honorifics.
    let s = s
        .trim_start_matches("hon. ")
        .trim_start_matches("hon ")
        .trim_start_matches("judge ")
        .trim();
    // Collapse internal whitespace.
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jurisdiction_tax_is_federal() {
        assert_eq!(map_courtlistener_jurisdiction("tax"), "us-federal");
        assert_eq!(map_courtlistener_jurisdiction("scotus"), "us-federal");
        assert_eq!(map_courtlistener_jurisdiction("ca9"), "us-federal");
    }

    #[test]
    fn canonical_court_names() {
        assert_eq!(canonical_court_name("tax"), "United States Tax Court");
        assert_eq!(
            canonical_court_name("unknown-court"),
            "Court (unknown-court)"
        );
    }

    #[test]
    fn extracts_judge_from_line_leading_pattern() {
        let line = "GOEKE, Judge: This case is before us on petitioner's";
        assert_eq!(extract_judges_from_line(line), vec!["GOEKE".to_string()]);
    }

    #[test]
    fn extracts_judge_from_initial_pattern() {
        let line = "Holmes, J., delivered the opinion of the Court.";
        assert_eq!(extract_judges_from_line(line), vec!["Holmes".to_string()]);
    }

    #[test]
    fn rejects_non_name_lines() {
        // Too many tokens
        assert!(extract_judges_from_line(
            "Some Random Words, Judge: appearing here"
        )
        .is_empty());
        // Empty + overly long lines
        assert!(extract_judges_from_line("").is_empty());
        assert!(extract_judges_from_line(&"x".repeat(250)).is_empty());
    }

    #[test]
    fn normalize_strips_honorifics() {
        assert_eq!(normalize_judge_name("Hon. Jane Smith"), "jane smith");
        assert_eq!(normalize_judge_name("Judge GOEKE"), "goeke");
        assert_eq!(normalize_judge_name("  Holmes  "), "holmes");
    }

    #[test]
    fn extract_judge_names_is_dedup_and_ordered() {
        // Same name twice → emitted once; different names preserve insertion order.
        let header = "
            GOEKE, Judge: This case is before us.
            Holmes, J., delivered the opinion of the Court.
            GOEKE, Judge: (reiterated)
        ";
        let names = extract_judge_names(header);
        assert_eq!(names, vec!["GOEKE".to_string(), "Holmes".to_string()]);
    }
}
