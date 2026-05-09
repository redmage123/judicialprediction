//! CourtListener REST API ingest path.
//!
//! Used when the bulk-data tarball endpoint is unavailable — e.g. when the
//! egress IP is in CourtListener's CloudFront WAF block list, which is the
//! current state for our Hetzner host (S3.6 finding).
//!
//! Two-stage strategy
//! ------------------
//! 1. Enumerate opinion IDs by court via `/api/rest/v4/search/?type=o&court=<id>`.
//!    The search endpoint is backed by Solr/Elasticsearch and returns 20
//!    results/page in well under a second. Pagination cursor in `next`.
//! 2. For each ID, fetch `/api/rest/v4/opinions/<id>/` to get the full
//!    `plain_text`. The `/opinions/` list endpoint with deep filters
//!    (e.g. `cluster__docket__court=<id>`) reliably times out at 60+ s
//!    on the upstream side — DO NOT use it.
//!
//! Rate limiting
//! -------------
//! Free-tier auth users get **5 requests per rolling 60-second window** on
//! `/opinions/<id>/` (and `/clusters/<id>/`). The search endpoint appears
//! to have a higher quota (no 429 observed during probing). We sleep
//! `OPINION_DELAY` (13 s) between opinion fetches to stay under the limit.
//!
//! At ~12 s per opinion, fetching 1000 opinions takes ~3.5 hours. Production
//! ingest at scale needs either:
//!   - A Free Law Project IP allowlist (contact https://free.law/contact/),
//!   - Or a CourtListener paid commercial subscription.
//!
//! Token: `$COURTLISTENER_TOKEN`. Get one from
//! https://www.courtlistener.com/profile/api/

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;

use crate::parse::Opinion;

const REST_BASE: &str = "https://www.courtlistener.com/api/rest/v4";
const SEARCH_PAGE_SIZE: u32 = 20; // /search/ ignores larger sizes
/// Stay under the 5/min hard limit on `/opinions/<id>/`.
const OPINION_DELAY: Duration = Duration::from_secs(13);
const SEARCH_DELAY: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize)]
struct SearchPage<T> {
    next: Option<String>,
    results: Vec<T>,
}

/// Subset of `/search/?type=o` result we need.
#[derive(Debug, Deserialize)]
struct SearchHit {
    /// Cluster id (case). Each cluster may have multiple sub-opinions.
    #[allow(dead_code)]
    cluster_id: i64,
    /// Sub-opinions of this cluster — IDs we hand off to `/opinions/<id>/`.
    opinions: Vec<SearchHitOpinion>,
    #[serde(rename = "caseName")]
    case_name: Option<String>,
    #[serde(rename = "dateFiled")]
    date_filed: Option<String>,
    #[serde(rename = "citeCount", default)]
    cite_count: i32,
    absolute_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchHitOpinion {
    id: i64,
}

/// Subset of `/opinions/<id>/` we need.
#[derive(Debug, Deserialize)]
struct OpinionDetail {
    id: i64,
    plain_text: Option<String>,
}

fn build_client(token: &str) -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Token {token}"))
            .context("invalid CourtListener token")?,
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "JudicialPredict-Ingest/0.1 (https://github.com/openclaw/judicialpredict)",
        ),
    );
    reqwest::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(30))
        .build()
        .context("build reqwest client")
}

/// Outcome of a single ingest run.
#[derive(Debug, Default, Clone)]
pub struct IngestStats {
    pub stored: usize,
    pub empty_text_skipped: u32,
    pub already_in_db_skipped: u32,
    pub http_errors: u32,
    /// True when we hit the daily-quota tier (e.g. 125/day) and stopped early.
    /// Caller should not retry today; tomorrow's run will continue.
    pub daily_cap_hit: bool,
}

/// Fetch + upsert up to `target_count` opinions for a given court.
///
/// Each opinion is upserted to Postgres immediately after being fetched, so
/// a daily-cap-induced exit mid-run still persists everything we got. Already-
/// ingested opinion_ids are skipped before hitting `/opinions/<id>/` (saves
/// quota — the search endpoint isn't bound by the 125/day cap).
///
/// Returns IngestStats on graceful completion. Bubbles up real errors (network,
/// DB connection failure) but treats per-opinion 4xx as `http_errors` and
/// continues. A 429 with daily-cap body sets `daily_cap_hit=true` and stops
/// the run cleanly so a long sleep doesn't tie up a cron slot.
pub async fn fetch_and_upsert_via_rest(
    pool: &sqlx::PgPool,
    token: &str,
    court: &str,
    target_count: usize,
) -> Result<IngestStats> {
    let client = build_client(token)?;
    let already_ingested = load_existing_opinion_ids(pool, court).await?;
    tracing::info!(
        already_in_db = already_ingested.len(),
        "loaded existing opinion_ids"
    );

    // Stage 1: enumerate IDs + case metadata via /search/. Search is not
    // bound by the 125/day cap, so we can scan more candidates than we
    // need and still find net-new opinions.
    let scan_size = (target_count * 4).max(40);
    let candidates = enumerate_via_search(&client, court, scan_size).await?;
    tracing::info!(candidates = candidates.len(), "stage 1 complete");

    // Stage 2: hydrate + upsert one-by-one. Returns early on daily cap.
    let mut stats = IngestStats::default();
    let mut hydrated_count = 0usize;
    for cand in candidates.iter() {
        if stats.stored >= target_count {
            break;
        }
        if already_ingested.contains(&cand.opinion_id) {
            stats.already_in_db_skipped += 1;
            continue;
        }
        if hydrated_count > 0 {
            tokio::time::sleep(OPINION_DELAY).await;
        }
        hydrated_count += 1;
        match hydrate_opinion(&client, cand).await {
            Ok(Some(op)) => {
                if let Err(e) = crate::db::upsert_opinions(pool, std::iter::once(op)).await {
                    tracing::warn!(error = %e, opinion_id = cand.opinion_id, "upsert failed");
                    stats.http_errors += 1;
                } else {
                    stats.stored += 1;
                }
            }
            Ok(None) => stats.empty_text_skipped += 1,
            Err(e) => {
                let msg = format!("{e:?}");
                stats.http_errors += 1;
                tracing::warn!(error = %e, opinion_id = cand.opinion_id, "skipped (HTTP error)");
                if msg.contains("/day") {
                    tracing::warn!(
                        "daily quota hit; ending run early (next run will continue)"
                    );
                    stats.daily_cap_hit = true;
                    break;
                }
            }
        }
        tracing::info!(
            stored = stats.stored,
            target = target_count,
            empty_text_skipped = stats.empty_text_skipped,
            already_in_db_skipped = stats.already_in_db_skipped,
            http_errors = stats.http_errors,
            "progress"
        );
    }

    Ok(stats)
}

/// Pre-load opinion_ids already in case_documents for this court so we can
/// skip them in the search scan without burning quota on /opinions/<id>/.
async fn load_existing_opinion_ids(
    pool: &sqlx::PgPool,
    court: &str,
) -> Result<std::collections::HashSet<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT opinion_id FROM case_documents WHERE court_id = $1",
    )
    .bind(court)
    .fetch_all(pool)
    .await
    .context("load existing opinion_ids")?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

#[derive(Debug, Clone)]
struct OpinionCandidate {
    opinion_id: i64,
    court_id: String,
    case_name: Option<String>,
    date_filed: Option<chrono::NaiveDate>,
    citation_count: i32,
    source_url: Option<String>,
}

async fn enumerate_via_search(
    client: &reqwest::Client,
    court: &str,
    target_count: usize,
) -> Result<Vec<OpinionCandidate>> {
    let mut next_url = Some(format!(
        "{REST_BASE}/search/?type=o&court={court}&format=json&page_size={SEARCH_PAGE_SIZE}"
    ));
    let mut out = Vec::with_capacity(target_count);
    let mut page = 0u32;
    while let Some(url) = next_url.take() {
        if out.len() >= target_count {
            break;
        }
        page += 1;
        tracing::info!(page, accumulated = out.len(), "search page");
        let resp = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "search returned {status}: {}",
                body.chars().take(200).collect::<String>()
            );
        }
        let parsed: SearchPage<SearchHit> =
            resp.json().await.context("decode search page")?;
        for hit in parsed.results {
            let date_filed = hit.date_filed.as_deref().and_then(parse_iso_date);
            for op in hit.opinions {
                out.push(OpinionCandidate {
                    opinion_id: op.id,
                    court_id: court.to_string(),
                    case_name: hit.case_name.clone(),
                    date_filed,
                    citation_count: hit.cite_count,
                    source_url: hit.absolute_url.clone(),
                });
                if out.len() >= target_count {
                    break;
                }
            }
            if out.len() >= target_count {
                break;
            }
        }
        next_url = parsed.next;
        if next_url.is_some() && out.len() < target_count {
            tokio::time::sleep(SEARCH_DELAY).await;
        }
    }
    Ok(out)
}

/// Returns Ok(Some(op)) on success, Ok(None) on empty plain_text, Err on
/// HTTP failure other than 429 (which is retried once with sleep).
async fn hydrate_opinion(
    client: &reqwest::Client,
    cand: &OpinionCandidate,
) -> Result<Option<Opinion>> {
    let url = format!("{REST_BASE}/opinions/{}/?format=json", cand.opinion_id);

    let mut resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;

    // 429 retry: CourtListener returns "Expected available in N seconds"
    // in the body. Parse it, sleep N+2 seconds (margin), then retry once.
    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let body = resp.text().await.unwrap_or_default();
        let wait = parse_retry_after_seconds(&body).unwrap_or(60);
        tracing::warn!(opinion_id = cand.opinion_id, wait_s = wait, "429 — backing off");
        tokio::time::sleep(Duration::from_secs(wait + 2)).await;
        resp = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url} (retry)"))?;
    }

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "opinion {} returned {status}: {}",
            cand.opinion_id,
            body.chars().take(120).collect::<String>()
        );
    }
    let detail: OpinionDetail = resp.json().await.context("decode opinion")?;
    let Some(text) = detail.plain_text else {
        return Ok(None);
    };
    if text.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(Opinion {
        opinion_id: detail.id,
        court_id: cand.court_id.clone(),
        case_name: cand.case_name.clone(),
        date_filed: cand.date_filed,
        citation_count: cand.citation_count,
        full_text_plain: text,
        source_url: cand.source_url.clone(),
    }))
}

/// Extract the seconds-to-wait hint from a CourtListener 429 body like
/// `{"detail":"Request was throttled. Rate limit exceeded: 5/min. Expected available in 6 seconds."}`.
fn parse_retry_after_seconds(body: &str) -> Option<u64> {
    // Cheap parse — find "in N seconds" without pulling in regex.
    let needle = "available in ";
    let idx = body.find(needle)?;
    let rest = &body[idx + needle.len()..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}

fn parse_iso_date(s: &str) -> Option<chrono::NaiveDate> {
    // CourtListener returns dates as "YYYY-MM-DDTHH:MM:SSZ" or "YYYY-MM-DD".
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.date_naive());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_hit_decodes_with_required_fields() {
        let json = r#"{
            "cluster_id": 7,
            "opinions": [{"id": 42}],
            "caseName": "Acme v. Tax Court",
            "dateFiled": "2024-06-15",
            "citeCount": 3,
            "absolute_url": "/opinion/7/"
        }"#;
        let hit: SearchHit = serde_json::from_str(json).unwrap();
        assert_eq!(hit.cluster_id, 7);
        assert_eq!(hit.opinions.len(), 1);
        assert_eq!(hit.case_name.as_deref(), Some("Acme v. Tax Court"));
        assert_eq!(hit.cite_count, 3);
    }

    #[test]
    fn search_hit_decodes_with_minimal_fields() {
        let json = r#"{"cluster_id":7,"opinions":[{"id":42}]}"#;
        let hit: SearchHit = serde_json::from_str(json).unwrap();
        assert!(hit.case_name.is_none());
        assert!(hit.date_filed.is_none());
        assert_eq!(hit.cite_count, 0);
    }

    #[test]
    fn opinion_detail_decodes_with_plain_text() {
        let json = r#"{"id":42,"plain_text":"hello tax court"}"#;
        let d: OpinionDetail = serde_json::from_str(json).unwrap();
        assert_eq!(d.id, 42);
        assert_eq!(d.plain_text.as_deref(), Some("hello tax court"));
    }

    #[test]
    fn parse_date_handles_both_formats() {
        assert_eq!(
            parse_iso_date("2024-06-15"),
            Some(chrono::NaiveDate::from_ymd_opt(2024, 6, 15).unwrap())
        );
        assert_eq!(
            parse_iso_date("2024-06-15T12:00:00Z"),
            Some(chrono::NaiveDate::from_ymd_opt(2024, 6, 15).unwrap())
        );
        assert!(parse_iso_date("not a date").is_none());
    }

    #[test]
    fn retry_after_parses_courtlistener_body() {
        let body = r#"{"detail":"Request was throttled. Rate limit exceeded: 5/min. Expected available in 6 seconds."}"#;
        assert_eq!(parse_retry_after_seconds(body), Some(6));
    }

    #[test]
    fn retry_after_returns_none_for_unrelated_body() {
        assert_eq!(parse_retry_after_seconds("404 not found"), None);
    }

    #[test]
    fn search_page_decodes_with_pagination() {
        let json = r#"{"count":13865,"next":"http://example/next","previous":null,"results":[]}"#;
        let p: SearchPage<SearchHit> = serde_json::from_str(json).unwrap();
        assert_eq!(p.next.as_deref(), Some("http://example/next"));
    }
}
