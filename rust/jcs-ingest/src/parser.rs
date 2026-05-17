//! Judicial Common Space CSV parser.
//!
//! Real Epstein/Martin/Quinn/Segal CSVs have a long tail of columns; we
//! pull just the four we need (name, court, score, judge id).  Header
//! detection is by name so future column-order shuffles don't break us.

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct JcsRow {
    /// "Last, First" or "First Last" — the matcher preprocesses both.
    pub judge_name: String,
    /// CourtListener-style slug ("scotus", "ca9", "txnd"), if present.
    /// JCS releases include the originating court for joint-scaling.
    #[serde(default)]
    pub court: String,
    /// One-dimensional JCS ideal-point. Roughly [-1, 1]; lower = more
    /// liberal. Scaled to [0, 1] by the gateway resolver before the model
    /// consumes it.
    #[serde(default, deserialize_with = "parse_optional_f64")]
    pub jcs: Option<f64>,
    /// Epstein's stable per-judge identifier.
    pub judge_id: String,
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

pub fn parse_jcs_csv(path: &Path) -> anyhow::Result<Vec<JcsRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)?;
    let mut rows = Vec::new();
    for (idx, result) in rdr.deserialize::<JcsRow>().enumerate() {
        match result {
            Ok(r) => rows.push(r),
            Err(e) => tracing::warn!(row = idx + 2, error = %e, "skipping malformed JCS row"),
        }
    }
    Ok(rows)
}
