//! Match a DIME row to a row in our `judges` table.
//!
//! Two-pass strategy (descending confidence):
//!
//!   1. **Exact** — `(normalized_name, primary_court_id)` matches a judge
//!      with a known `primary_court_id`. We trust the court linkage from the
//!      ingest-fetcher KG build, so this is the strongest match.
//!   2. **Name-only** — `normalized_name` matches a judge with NO
//!      `primary_court_id` set yet (a CourtListener opinion mentioned them
//!      but we haven't tied them to a court).  Lower confidence; we still
//!      write but tag the match accordingly.
//!
//! Anything else is skipped and surfaces in the `--report` file. We
//! deliberately don't fuzzy-match (Levenshtein, soundex, etc.) — silent
//! over-matching would write the wrong cfscore to the wrong judge, which
//! is exactly the failure mode the compliance disclosure has to defend
//! against.

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::parser::{dime_name_to_last_token, dime_name_to_match_key, DimeRow};

/// How sure are we that this DIME row corresponds to the judge we matched?
/// Persisted into `bio.dime.match_confidence` so audit + UI can show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchConfidence {
    /// `(normalized_name, primary_court_id)` hit one row.
    Exact,
    /// `normalized_name` hit one row that had no `primary_court_id`.
    NameOnly,
    /// Last-name only + court matched. Used because ingest-fetcher's
    /// opinion-header extractor commonly stores `judges.normalized_name`
    /// as a single last-name token (e.g. `"tannenwald"`), so the full
    /// "first last" key produced from DIME doesn't hit; the last-token
    /// key does, and the court id disambiguates.
    LastNameAndCourt,
}

impl MatchConfidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::NameOnly => "name_only",
            Self::LastNameAndCourt => "last_name+court",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub judge_id: Uuid,
    pub normalized_name: String,
    pub confidence: MatchConfidence,
}

/// Look the row up in the judges table for `tenant_id`.
///
/// Returns `Ok(None)` for a clean miss (no judge by that name on that
/// court), `Ok(Some(_))` for a hit, and propagates DB errors. Multiple-row
/// hits — same normalised name on the same court — are treated as a
/// reportable miss; we don't pick a winner.
pub async fn match_row(
    pool: &PgPool,
    tenant_id: Uuid,
    row: &DimeRow,
) -> anyhow::Result<Option<MatchResult>> {
    let normalized = dime_name_to_match_key(&row.name);
    if normalized.is_empty() {
        return Ok(None);
    }

    // RLS scope so subsequent queries see only this tenant's judges.
    let mut tx = pool.begin().await?;
    sqlx::query(&format!("SET LOCAL app.current_tenant_id = '{tenant_id}'"))
        .execute(&mut *tx)
        .await?;

    // Pass 1 — exact (name + court).  Only run when the row carries a
    // court slug; we resolve it to the courts.id via courts.source_id
    // (which is the CourtListener slug — see rust/feature-store/migrations
    // baseline.  The column is called `source_id`, not `slug`, because the
    // KG keeps the upstream identifier verbatim).
    if !row.court.trim().is_empty() {
        let hits = sqlx::query(
            r#"
            SELECT j.id
              FROM judges j
              JOIN courts c ON c.id = j.primary_court_id
             WHERE j.tenant_id = $1
               AND j.normalized_name = $2
               AND c.source_id = $3
            "#,
        )
        .bind(tenant_id)
        .bind(&normalized)
        .bind(row.court.trim().to_lowercase())
        .fetch_all(&mut *tx)
        .await?;

        if hits.len() == 1 {
            let judge_id: Uuid = hits[0].try_get("id")?;
            tx.commit().await?;
            return Ok(Some(MatchResult {
                judge_id,
                normalized_name: normalized,
                confidence: MatchConfidence::Exact,
            }));
        } else if hits.len() > 1 {
            // Ambiguous court+name — pretend it's a miss for safety.
            tracing::warn!(
                normalized = %normalized,
                court = %row.court,
                count = hits.len(),
                "ambiguous court+name match; skipping"
            );
            tx.commit().await?;
            return Ok(None);
        }
    }

    // Pass 2 — name-only, but only when the candidate judge has no
    // primary_court_id (otherwise pass 1 would have caught a hit, or this
    // is a name collision with a different judge on a different court).
    let hits = sqlx::query(
        r#"
        SELECT id
          FROM judges
         WHERE tenant_id = $1
           AND normalized_name = $2
           AND primary_court_id IS NULL
        "#,
    )
    .bind(tenant_id)
    .bind(&normalized)
    .fetch_all(&mut *tx)
    .await?;

    if hits.len() == 1 {
        let judge_id: Uuid = hits[0].try_get("id")?;
        tx.commit().await?;
        return Ok(Some(MatchResult {
            judge_id,
            normalized_name: normalized,
            confidence: MatchConfidence::NameOnly,
        }));
    } else if hits.len() > 1 {
        tracing::warn!(
            normalized = %normalized,
            count = hits.len(),
            "ambiguous name-only match; skipping"
        );
        tx.commit().await?;
        return Ok(None);
    }

    // Pass 3 — last-name + court. ingest-fetcher stores
    // judges.normalized_name as a single last-name token when the opinion
    // header has "TANNENWALD, Judge:" form. The full "first last" key from
    // DIME doesn't hit; the last token does. We require a court match here
    // so two "Smiths" on different courts don't both light up.
    let last_token = dime_name_to_last_token(&row.name);
    if !last_token.is_empty() && !row.court.trim().is_empty() && last_token != normalized {
        let hits = sqlx::query(
            r#"
            SELECT j.id
              FROM judges j
              JOIN courts c ON c.id = j.primary_court_id
             WHERE j.tenant_id = $1
               AND j.normalized_name = $2
               AND c.source_id = $3
            "#,
        )
        .bind(tenant_id)
        .bind(&last_token)
        .bind(row.court.trim().to_lowercase())
        .fetch_all(&mut *tx)
        .await?;
        if hits.len() == 1 {
            let judge_id: Uuid = hits[0].try_get("id")?;
            tx.commit().await?;
            return Ok(Some(MatchResult {
                judge_id,
                normalized_name: last_token,
                confidence: MatchConfidence::LastNameAndCourt,
            }));
        } else if hits.len() > 1 {
            tracing::warn!(
                last_token = %last_token,
                court = %row.court,
                count = hits.len(),
                "ambiguous last-name+court match; skipping"
            );
        }
    }

    tx.commit().await?;
    Ok(None)
}
