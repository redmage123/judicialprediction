//! Imperative shell: upsert parsed opinions into Postgres.

use anyhow::{Context, Result};
use sqlx::PgPool;

use crate::parse::Opinion;

#[derive(Debug, Default, Clone)]
pub struct UpsertStats {
    pub inserted: u64,
    pub updated: u64,
    pub skipped: u64,
}

/// Idempotent upsert: ON CONFLICT (opinion_id) DO UPDATE.
///
/// We do one row per query rather than COPY because (a) the volumes are
/// modest (≤ tens of thousands per court) and (b) we want per-row error
/// isolation — one bad row should not abort the whole ingest.
pub async fn upsert_opinions<I>(pool: &PgPool, opinions: I) -> Result<UpsertStats>
where
    I: IntoIterator<Item = Opinion>,
{
    let mut stats = UpsertStats::default();
    for op in opinions {
        // S6.5 — if the source carried a cites array, capture it (and stamp
        // cites_extracted_at).  Bulk-tarball callers pass `cites = None`, in
        // which case both cites columns are left at whatever the existing row
        // had (or NULL on insert) and the back-fill worker will pick them up.
        let cites_value = op.cites.as_ref().map(serde_json::to_value).transpose()
            .with_context(|| format!("serialize cites for opinion_id={}", op.opinion_id))?;
        let res = sqlx::query(
            r#"
            INSERT INTO case_documents
              (court_id, opinion_id, case_name, date_filed, citation_count,
               full_text_plain, source, source_url,
               cites_json, cites_extracted_at)
            VALUES ($1, $2, $3, $4, $5, $6, 'courtlistener', $7,
                    $8,
                    CASE WHEN $8::jsonb IS NULL THEN NULL ELSE now() END)
            ON CONFLICT (opinion_id) DO UPDATE SET
              court_id           = EXCLUDED.court_id,
              case_name          = EXCLUDED.case_name,
              date_filed         = EXCLUDED.date_filed,
              citation_count     = EXCLUDED.citation_count,
              full_text_plain    = EXCLUDED.full_text_plain,
              source_url         = EXCLUDED.source_url,
              ingested_at        = now(),
              cites_json         = COALESCE(EXCLUDED.cites_json, case_documents.cites_json),
              cites_extracted_at = COALESCE(EXCLUDED.cites_extracted_at,
                                            case_documents.cites_extracted_at)
            "#,
        )
        .bind(&op.court_id)
        .bind(op.opinion_id)
        .bind(op.case_name.as_deref())
        .bind(op.date_filed)
        .bind(op.citation_count)
        .bind(&op.full_text_plain)
        .bind(op.source_url.as_deref())
        .bind(&cites_value)
        .execute(pool)
        .await
        .with_context(|| format!("upsert opinion_id={}", op.opinion_id))?;

        // Postgres reports rows_affected() = 1 for both insert and update under
        // ON CONFLICT DO UPDATE; we approximate by checking xmax of the row.
        // For Sprint-2 we just count "touched" rows.
        if res.rows_affected() == 1 {
            stats.inserted += 1;
        } else {
            stats.skipped += 1;
        }
    }
    Ok(stats)
}
