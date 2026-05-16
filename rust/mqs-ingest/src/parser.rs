//! Martin-Quinn judge CSV parser.
//!
//! Row shape (subset we use; extra columns ignored):
//!
//!   justice_name,term,post_mean,post_sd,justiceID
//!
//! Where `term` is the four-digit court term year (1937..most-recent),
//! `post_mean` is the posterior mean of the dynamic ideal-point in
//! one-dimensional space (negative = liberal), `post_sd` is its
//! posterior standard deviation, and `justiceID` is Bonica/Spaeth's
//! stable per-justice identifier.

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct MqsRow {
    /// "Last, First" form. Same as DIME's `name` column.
    #[serde(rename = "justice_name")]
    pub justice_name: String,
    /// Four-digit court term year.
    pub term: i32,
    /// Posterior mean of the ideal-point. Negative = liberal,
    /// positive = conservative. Roughly [-6, 6] with most rows
    /// in [-3, 3].
    #[serde(default, deserialize_with = "parse_optional_f64")]
    pub post_mean: Option<f64>,
    /// Posterior standard deviation. Optional in pre-1937 / sparse
    /// terms; we preserve it for the audit trail but the model never
    /// consumes it directly.
    #[serde(default, deserialize_with = "parse_optional_f64")]
    pub post_sd: Option<f64>,
    /// Bonica/Spaeth's per-justice stable identifier.
    #[serde(rename = "justiceID")]
    pub justice_id: String,
}

fn parse_optional_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Option<String> = Option::deserialize(deserializer)?;
    Ok(raw.and_then(|s| {
        let t = s.trim();
        if t.is_empty() || t.eq_ignore_ascii_case("na") || t.eq_ignore_ascii_case("nan") {
            None
        } else {
            t.parse::<f64>().ok()
        }
    }))
}

/// Parse a Martin-Quinn CSV. Header-mismatch and IO errors propagate;
/// individual malformed rows are warned and skipped.
pub fn parse_mqs_csv(path: &Path) -> anyhow::Result<Vec<MqsRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)?;

    let mut rows = Vec::new();
    for (idx, result) in rdr.deserialize::<MqsRow>().enumerate() {
        match result {
            Ok(r) => rows.push(r),
            Err(e) => {
                tracing::warn!(row = idx + 2, error = %e, "skipping malformed MQ row");
            }
        }
    }
    Ok(rows)
}
