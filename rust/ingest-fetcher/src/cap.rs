//! Sprint 15 / S15.5 — Caselaw Access Project bulk ingest.
//!
//! Streams per-case JSONs from the Free Law Project static.case.law mirror
//! into `case_documents` with `source = 'cap'`. Federal slice only for this
//! sprint; state-court ingest is a Sprint 16 ask.
//!
//! Discovery strategy
//! ------------------
//! static.case.law is a plain-HTTP directory tree:
//!   `https://static.case.law/<jurisdiction>/<volume>/cases/<file>.json`
//!
//! Each level is an HTML listing of links. We walk:
//!   1. `/<jurisdiction>/` → list of volume numbers
//!   2. `/<jurisdiction>/<volume>/cases/` → list of `*.json` files
//!   3. fetch each case JSON, project to a `CapRow`, INSERT.
//!
//! No bulk zip download. The 6.7M-row corpus is too big for the dev box;
//! we cap this run at `limit` and stream one case at a time. Memory cost
//! stays under a few MB regardless of corpus size.
//!
//! Fallback contract
//! -----------------
//! If `static.case.law` is unreachable (DNS, TLS, 5xx, rate-limit), this
//! module logs a clear warning and exits with `ingested = 0`. Callers (the
//! CLI + cron) must treat this as a clean shutdown — no panic, no
//! propagated error. S15.4/S15.8 run in parallel and must not be blocked
//! by a CAP outage.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::PgPool;

const CAP_BASE: &str = "https://static.case.law";
/// Per-request timeout. The static mirror is fast (Cloudflare-fronted)
/// but occasional 200-byte HTML listings rarely take >2 s; bound at 30
/// to be safe against transient network blips.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Polite delay between case-JSON fetches. The mirror is static (no
/// quota documented) but a tiny sleep keeps us off any anti-abuse radar
/// without measurably slowing a 30k-opinion run.
const PER_CASE_DELAY: Duration = Duration::from_millis(50);

#[derive(Debug, Default, Clone)]
pub struct CapStats {
    /// Rows where INSERT actually persisted a new row.
    pub ingested: u64,
    /// Rows skipped because `opinion_id` already existed (UNIQUE conflict).
    pub skipped: u64,
    /// Bad JSON, missing required fields, or per-row DB failures.
    /// Errors are logged and counted; the run continues.
    pub errors: u64,
}

/// Subset of the CAP case JSON we actually need.
///
/// CAP's shape (post-FLP takeover) is documented at
/// <https://case.law/docs/site_features/api>. Fields we keep:
///   - `id`       → `case_documents.opinion_id`
///   - `name`     → `case_documents.case_name`
///   - `decision_date` → `case_documents.date_filed`
///   - `casebody.opinions[].text` → concatenated into `full_text_plain`
///
/// Everything else (citations, analysis blob, jurisdiction id, etc.) is
/// out of scope for this ticket and dropped at parse time.
#[derive(Debug, Deserialize)]
struct CapCase {
    id: i64,
    name: Option<String>,
    decision_date: Option<String>,
    /// `court` may be absent on a handful of pre-1800 anomaly records.
    /// We tolerate missing values and fall back to the URL-level slug
    /// (`jurisdiction`) for routing. Field is kept for forward
    /// compatibility / future enrichment but is not read today.
    #[serde(default)]
    #[allow(dead_code)]
    court: Option<CapCourt>,
    casebody: Option<CapCaseBody>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CapCourt {
    /// CAP's internal numeric id (e.g. 9009 for SCOTUS). We don't use it
    /// for routing — the URL slug (`us`, `f3d`, ...) maps onto the
    /// detector's dispatch table more cleanly.
    id: Option<serde_json::Value>,
    name_abbreviation: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CapCaseBody {
    data: Option<CapCaseBodyData>,
    /// Newer CAP exports flatten the body — `opinions` may live at the
    /// `casebody` root rather than under `data`. We tolerate both.
    #[serde(default)]
    opinions: Vec<CapOpinion>,
}

#[derive(Debug, Deserialize)]
struct CapCaseBodyData {
    #[serde(default)]
    opinions: Vec<CapOpinion>,
}

#[derive(Debug, Deserialize)]
struct CapOpinion {
    #[allow(dead_code)]
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

/// Projection target: one row ready to INSERT into `case_documents`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapRow {
    pub opinion_id: i64,
    pub court_id: String,
    pub case_name: Option<String>,
    pub date_filed: Option<chrono::NaiveDate>,
    pub full_text_plain: String,
    pub source_url: Option<String>,
}

/// Pure projection: CAP JSON + URL context → `CapRow`.
///
/// Separated from the HTTP and DB code so unit tests can exercise the
/// schema mapping without a network or a Postgres.
fn project_case(case: CapCase, jurisdiction: &str, source_url: &str) -> Option<CapRow> {
    // Concatenate every opinion text in the order CAP gave them.
    // Empty opinions skip — `full_text_plain` is NOT NULL.
    let mut opinions = case
        .casebody
        .as_ref()
        .and_then(|cb| cb.data.as_ref())
        .map(|d| d.opinions.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|op| op.text.as_deref().map(str::trim).filter(|t| !t.is_empty()))
        .map(|t| t.to_string())
        .collect::<Vec<_>>();

    // Tolerate the flat shape too.
    if opinions.is_empty() {
        opinions = case
            .casebody
            .as_ref()
            .map(|cb| cb.opinions.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter_map(|op| op.text.as_deref().map(str::trim).filter(|t| !t.is_empty()))
            .map(|t| t.to_string())
            .collect::<Vec<_>>();
    }

    if opinions.is_empty() {
        return None;
    }
    let full_text_plain = opinions.join("\n\n");

    // CAP's `decision_date` can be `YYYY-MM-DD`, `YYYY-MM`, or just `YYYY`.
    // We try the strict format first; partial dates parse as the first day
    // of the period. Unparseable → `None`, which `case_documents` accepts.
    let date_filed = case.decision_date.as_deref().and_then(parse_cap_date);

    Some(CapRow {
        opinion_id: case.id,
        court_id: jurisdiction.to_string(),
        case_name: case.name,
        date_filed,
        full_text_plain,
        source_url: Some(source_url.to_string()),
    })
}

/// Parse CAP's permissive date strings. Handles `YYYY`, `YYYY-MM`,
/// `YYYY-MM-DD`. Missing components default to January 1st.
fn parse_cap_date(s: &str) -> Option<chrono::NaiveDate> {
    use chrono::NaiveDate;
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    if let Ok(d) = NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d") {
        return Some(d);
    }
    if let Ok(d) = NaiveDate::parse_from_str(&format!("{s}-01-01"), "%Y-%m-%d") {
        return Some(d);
    }
    None
}

/// Insert a single row with `ON CONFLICT DO NOTHING`. Returns:
///   - `Ok(true)`  if a new row was persisted,
///   - `Ok(false)` if the opinion_id already existed.
async fn insert_row(pool: &PgPool, row: &CapRow) -> Result<bool> {
    let result = sqlx::query(
        r#"
        INSERT INTO case_documents
            (court_id, opinion_id, case_name, date_filed, citation_count,
             full_text_plain, source, source_url)
        VALUES ($1, $2, $3, $4, 0, $5, 'cap', $6)
        ON CONFLICT (opinion_id) DO NOTHING
        "#,
    )
    .bind(&row.court_id)
    .bind(row.opinion_id)
    .bind(row.case_name.as_deref())
    .bind(row.date_filed)
    .bind(&row.full_text_plain)
    .bind(row.source_url.as_deref())
    .execute(pool)
    .await
    .with_context(|| format!("insert opinion_id={}", row.opinion_id))?;

    Ok(result.rows_affected() == 1)
}

/// Build the shared HTTP client.
fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(
            "JudicialPredict-CAP-Ingest/0.1 (https://github.com/openclaw/judicialpredict)",
        )
        .build()
        .context("build reqwest client")
}

/// Crude HTML scraper for the static.case.law directory listings.
///
/// The pages are auto-generated and look like:
///   `<a href='https://static.case.law/us/1/cases/0001-01.json'>0001-01.json</a>`
/// We grep for `href='...'` tokens that point under the same prefix and
/// match a suffix filter. This is intentionally fragile-but-simple — if
/// the listing format changes, the regex breaks loudly rather than
/// silently dropping rows.
fn extract_hrefs(html: &str, prefix: &str, suffix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = "href='";
    let mut cursor = 0usize;
    while let Some(idx) = html[cursor..].find(needle) {
        let start = cursor + idx + needle.len();
        let Some(end_off) = html[start..].find('\'') else {
            break;
        };
        let end = start + end_off;
        let url = &html[start..end];
        if url.starts_with(prefix) && url.ends_with(suffix) {
            out.push(url.to_string());
        }
        cursor = end + 1;
    }
    out
}

/// List the volume numbers available under a jurisdiction.
async fn list_volumes(client: &reqwest::Client, jurisdiction: &str) -> Result<Vec<u32>> {
    let url = format!("{CAP_BASE}/{jurisdiction}/");
    let html = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx from {url}"))?
        .text()
        .await
        .with_context(|| format!("read body from {url}"))?;

    let prefix = format!("{CAP_BASE}/{jurisdiction}/");
    let hrefs = extract_hrefs(&html, &prefix, "/");
    // Each href looks like `{CAP_BASE}/{jurisdiction}/{vol}/`. Extract the
    // numeric volume; drop non-numeric entries (the metadata.json links).
    let mut vols: Vec<u32> = hrefs
        .iter()
        .filter_map(|href| {
            let rest = href.strip_prefix(&prefix)?.trim_end_matches('/');
            rest.parse::<u32>().ok()
        })
        .collect();
    vols.sort_unstable();
    vols.dedup();
    Ok(vols)
}

/// List the per-case JSON URLs in a single volume's `cases/` directory.
async fn list_case_urls(
    client: &reqwest::Client,
    jurisdiction: &str,
    volume: u32,
) -> Result<Vec<String>> {
    let url = format!("{CAP_BASE}/{jurisdiction}/{volume}/cases/");
    let html = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx from {url}"))?
        .text()
        .await
        .with_context(|| format!("read body from {url}"))?;

    let prefix = format!("{CAP_BASE}/{jurisdiction}/{volume}/cases/");
    let mut urls = extract_hrefs(&html, &prefix, ".json");
    urls.sort();
    urls.dedup();
    Ok(urls)
}

/// Fetch + parse one case JSON.
async fn fetch_case(client: &reqwest::Client, url: &str) -> Result<CapCase> {
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx from {url}"))?;
    let bytes = resp
        .bytes()
        .await
        .with_context(|| format!("read body from {url}"))?;
    serde_json::from_slice::<CapCase>(&bytes)
        .with_context(|| format!("decode CAP case JSON from {url}"))
}

/// Run the ingest against `pool`. Streams up to `limit` opinions across
/// `jurisdictions`, in the order given. Returns a `CapStats` summary.
///
/// **Never panics** on a network failure. If `static.case.law` is
/// unreachable, the run logs the failure, ingests zero rows, and returns
/// `Ok(stats)` with `errors > 0`. The caller decides whether that is OK
/// (S15 plan says it is — CAP is one of four sources).
pub async fn run_ingest(
    pool: &PgPool,
    jurisdictions: &[String],
    limit: usize,
) -> Result<CapStats> {
    let client = build_client()?;
    let mut stats = CapStats::default();

    for jurisdiction in jurisdictions {
        if (stats.ingested + stats.skipped) as usize >= limit {
            break;
        }
        let volumes = match list_volumes(&client, jurisdiction).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    jurisdiction = %jurisdiction,
                    "skipping jurisdiction: directory listing unreachable"
                );
                stats.errors += 1;
                continue;
            }
        };
        tracing::info!(
            jurisdiction = %jurisdiction,
            volumes = volumes.len(),
            "discovered volumes"
        );

        for volume in volumes {
            if (stats.ingested + stats.skipped) as usize >= limit {
                break;
            }
            let case_urls = match list_case_urls(&client, jurisdiction, volume).await {
                Ok(u) => u,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        jurisdiction = %jurisdiction,
                        volume = volume,
                        "skipping volume: case listing unreachable"
                    );
                    stats.errors += 1;
                    continue;
                }
            };

            for url in case_urls {
                if (stats.ingested + stats.skipped) as usize >= limit {
                    break;
                }
                let case = match fetch_case(&client, &url).await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(error = %e, url = %url, "skipping bad case JSON");
                        stats.errors += 1;
                        continue;
                    }
                };
                let Some(row) = project_case(case, jurisdiction, &url) else {
                    // No opinion text — not an error, just an empty record
                    // (e.g. a stub page). Don't count as error.
                    continue;
                };
                match insert_row(pool, &row).await {
                    Ok(true) => stats.ingested += 1,
                    Ok(false) => stats.skipped += 1,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            opinion_id = row.opinion_id,
                            "insert failed"
                        );
                        stats.errors += 1;
                    }
                }
                tokio::time::sleep(PER_CASE_DELAY).await;
            }
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal fixture: a SCOTUS case with two opinions (majority + dissent).
    const FIXTURE_SCOTUS: &str = r#"{
        "id": 4644,
        "name": "Ex parte French",
        "decision_date": "1879-10",
        "court": {
            "id": 9009,
            "name_abbreviation": "U.S.",
            "name": "Supreme Court of the United States"
        },
        "casebody": {
            "data": {
                "opinions": [
                    { "type": "majority", "text": "  majority body  " },
                    { "type": "dissent",  "text": "dissent body" }
                ]
            }
        }
    }"#;

    /// Flat shape — `opinions` lives at the casebody root, not under `data`.
    const FIXTURE_FLAT: &str = r#"{
        "id": 12345,
        "name": "Flat v. Shape",
        "decision_date": "2010-06-15",
        "court": { "id": 1, "name_abbreviation": "F.3d" },
        "casebody": {
            "opinions": [ { "type": "majority", "text": "flat body" } ]
        }
    }"#;

    /// Stub with no opinion text — projection must return None.
    const FIXTURE_EMPTY: &str = r#"{
        "id": 999,
        "name": "Empty",
        "decision_date": "1900",
        "court": { "id": 1, "name_abbreviation": "X" },
        "casebody": { "data": { "opinions": [] } }
    }"#;

    #[test]
    fn project_scotus_concatenates_opinions() {
        let case: CapCase = serde_json::from_str(FIXTURE_SCOTUS).unwrap();
        let row =
            project_case(case, "us", "https://static.case.law/us/100/cases/0001-01.json")
                .expect("project ok");
        assert_eq!(row.opinion_id, 4644);
        assert_eq!(row.court_id, "us");
        assert_eq!(row.case_name.as_deref(), Some("Ex parte French"));
        // Partial date `1879-10` → first of the month.
        assert_eq!(
            row.date_filed,
            Some(chrono::NaiveDate::from_ymd_opt(1879, 10, 1).unwrap())
        );
        assert!(row.full_text_plain.contains("majority body"));
        assert!(row.full_text_plain.contains("dissent body"));
        assert_eq!(
            row.source_url.as_deref(),
            Some("https://static.case.law/us/100/cases/0001-01.json")
        );
    }

    #[test]
    fn project_flat_shape() {
        let case: CapCase = serde_json::from_str(FIXTURE_FLAT).unwrap();
        let row = project_case(case, "f3d", "https://example.test/x.json").expect("ok");
        assert_eq!(row.opinion_id, 12345);
        assert_eq!(row.full_text_plain, "flat body");
        assert_eq!(
            row.date_filed,
            Some(chrono::NaiveDate::from_ymd_opt(2010, 6, 15).unwrap())
        );
    }

    #[test]
    fn project_empty_returns_none() {
        let case: CapCase = serde_json::from_str(FIXTURE_EMPTY).unwrap();
        assert!(project_case(case, "us", "x").is_none());
    }

    #[test]
    fn parse_cap_date_handles_partial() {
        use chrono::NaiveDate;
        assert_eq!(
            parse_cap_date("1879-10-15"),
            Some(NaiveDate::from_ymd_opt(1879, 10, 15).unwrap())
        );
        assert_eq!(
            parse_cap_date("1879-10"),
            Some(NaiveDate::from_ymd_opt(1879, 10, 1).unwrap())
        );
        assert_eq!(
            parse_cap_date("1879"),
            Some(NaiveDate::from_ymd_opt(1879, 1, 1).unwrap())
        );
        assert_eq!(parse_cap_date("not-a-date"), None);
    }

    #[test]
    fn extract_hrefs_filters_by_prefix_and_suffix() {
        let html = r#"
            <a href='https://static.case.law/us/1/cases/0001-01.json'>x</a>
            <a href='https://static.case.law/us/1/cases/0002-01.json'>y</a>
            <a href='https://static.case.law/us/'>parent</a>
            <a href='https://other.example/foo.json'>other</a>
        "#;
        let urls = extract_hrefs(html, "https://static.case.law/us/1/cases/", ".json");
        assert_eq!(urls.len(), 2);
        assert!(urls[0].ends_with("0001-01.json"));
        assert!(urls[1].ends_with("0002-01.json"));
    }
}
