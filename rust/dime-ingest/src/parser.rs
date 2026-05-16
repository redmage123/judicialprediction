//! Bonica DIME judge CSV parser.
//!
//! The judge release has the columns we care about plus a long tail we ignore.
//! We pull just the four we need; anything missing on a row is treated as
//! "skip this row, not a fatal error" so the import tolerates partial drops.
//!
//! Real Bonica CSVs use commas inside quoted strings (esp. for "Last, First"
//! name form). The `csv` crate handles that for us; no hand-rolled splitting.

use serde::Deserialize;
use std::path::Path;

/// One row from the DIME judge CSV that we care about.  Field names match
/// Bonica's published header for the 2014 judge release; extra columns in
/// the file are ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct DimeRow {
    /// Bonica's stable identifier for this judge entry, used so we can
    /// distinguish re-ingests of the same judge from a name conflict.
    #[serde(rename = "bonica_id")]
    pub bonica_id: String,

    /// Judge name. DIME uses "Last, First Middle" form. The matcher
    /// normalises to "first middle last" before lookup.
    pub name: String,

    /// Lower-cased court slug matching `courts.slug` (e.g. "tax", "scotus").
    /// May be empty for state-court judges in pre-2014 drops; matcher
    /// falls back to name-only when this is empty.
    #[serde(default)]
    pub court: String,

    /// Campaign-finance ideology score. Roughly [-2.0, 2.0], lower = more
    /// liberal. Bonica reports missing values as empty string; we deserialise
    /// to Option<f64> via parse_optional_f64.
    #[serde(deserialize_with = "parse_optional_f64", default)]
    pub cfscore: Option<f64>,
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

/// Parse a Bonica DIME judges CSV into the subset of fields we use.
///
/// Returns every row that deserialises cleanly. Header-mismatch and IO
/// errors propagate as `Err`. Individual malformed rows are reported via
/// `tracing::warn!` and skipped — we don't want one bad row aborting a
/// 4000-row import.
pub fn parse_dime_csv(path: &Path) -> anyhow::Result<Vec<DimeRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true) // tolerate trailing-column drift across releases
        .from_path(path)?;

    let mut rows = Vec::new();
    for (idx, result) in rdr.deserialize::<DimeRow>().enumerate() {
        match result {
            Ok(r) => rows.push(r),
            Err(e) => {
                tracing::warn!(row = idx + 2, error = %e, "skipping malformed DIME row");
            }
        }
    }
    Ok(rows)
}

// ─────────────────────────────────────────────────────────────────────────────
//  Name normalisation
// ─────────────────────────────────────────────────────────────────────────────
//
// DIME stores names as "Last, First Middle [Suffix]". We need to produce the
// same shape ingest-fetcher writes into `judges.normalized_name`, which is
// lowercased "first middle last" with honorifics stripped and whitespace
// collapsed (see ingest_fetcher::kg::normalize_judge_name).
//
// We do the comma swap + suffix strip locally, then DELEGATE to
// `ingest_fetcher::normalize_judge_name` for the final shaping. That keeps
// the two normalisers in lockstep — if ingest-fetcher's normaliser changes,
// DIME matching stays consistent.

/// Convert a DIME-format name to the same normalised string
/// `ingest_fetcher::normalize_judge_name` produces for opinion-extracted
/// judges.
pub fn dime_name_to_match_key(raw: &str) -> String {
    let cleaned = preprocess_dime_name(raw);
    ingest_fetcher::normalize_judge_name(&cleaned)
}

/// Last-name-only key for fallback matching.
///
/// CourtListener opinion headers commonly use a single uppercase last name
/// (`TANNENWALD, Judge:`), which `ingest_fetcher::extract_judge_names` +
/// `normalize_judge_name` reduce to a single-token `"tannenwald"` and store
/// in `judges.normalized_name`. DIME stores full names, so the full-name
/// key from `dime_name_to_match_key` won't match. This helper produces the
/// last-name token so the matcher can try a lower-confidence fallback.
pub fn dime_name_to_last_token(raw: &str) -> String {
    let cleaned = preprocess_dime_name(raw);
    // Last whitespace-delimited token after preprocessing.
    let last = cleaned
        .split_whitespace()
        .last()
        .unwrap_or("")
        .to_string();
    ingest_fetcher::normalize_judge_name(&last)
}

/// "Last, First Middle, Jr." → "First Middle Last".
///
/// Strips trailing suffixes (Jr/Sr/II/III/IV) and middle initials. We choose
/// to strip middle initials because CourtListener opinion headers often omit
/// them, and we'd rather under-match (and flag for human review) than
/// over-match (and write the wrong cfscore to a judge).
fn preprocess_dime_name(raw: &str) -> String {
    // Split on the FIRST comma — anything past that is given names + suffix.
    let (last, rest) = match raw.split_once(',') {
        Some((l, r)) => (l.trim().to_string(), r.trim()),
        None => return raw.to_string(),
    };

    // Drop a trailing "Jr."/"Sr."/"II"/"III"/"IV" if present.
    let rest = strip_suffix(rest);
    // Drop middle initials of the form "X." or single letters.
    let given = rest
        .split_whitespace()
        .filter(|tok| !is_middle_initial(tok))
        .collect::<Vec<_>>()
        .join(" ");

    if given.is_empty() {
        last
    } else {
        format!("{given} {last}")
    }
}

fn strip_suffix(s: &str) -> String {
    let suffixes = ["jr.", "jr", "sr.", "sr", "ii", "iii", "iv"];
    let lower = s.to_lowercase();
    for suf in &suffixes {
        // Match as a whole trailing token, comma-delimited or space-delimited.
        for delim in [",", " "] {
            let probe = format!("{delim}{suf}");
            if lower.ends_with(&probe) || lower == *suf {
                return s[..s.len() - probe.len().min(s.len())]
                    .trim_end_matches(|c: char| c == ',' || c.is_whitespace())
                    .to_string();
            }
        }
    }
    s.to_string()
}

fn is_middle_initial(tok: &str) -> bool {
    let t = tok.trim_end_matches('.');
    t.len() == 1 && t.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_last_first_to_match_ingest_fetcher() {
        assert_eq!(dime_name_to_match_key("Smith, John D."), "john smith");
        assert_eq!(
            dime_name_to_match_key("O'Connor, Sandra Day"),
            "sandra day o'connor"
        );
    }

    #[test]
    fn strips_suffixes() {
        assert_eq!(dime_name_to_match_key("Smith, John D., Jr."), "john smith");
        assert_eq!(dime_name_to_match_key("Smith, John III"), "john smith");
    }

    #[test]
    fn handles_no_comma() {
        // Some pre-2014 rows are already in "First Last" form.
        assert_eq!(dime_name_to_match_key("John Smith"), "john smith");
    }

    #[test]
    fn handles_lonely_last_name() {
        // Garbage row — Bonica sometimes ships these. We don't crash; we
        // produce a lookup key the matcher will fail to match (and report).
        assert_eq!(dime_name_to_match_key("Smith,"), "smith");
    }

    #[test]
    fn drops_middle_initial_only_not_short_first_name() {
        // "Jay" shouldn't be eaten as a middle initial.
        assert_eq!(
            dime_name_to_match_key("Pritchard, Jay Roy"),
            "jay roy pritchard"
        );
    }

    #[test]
    fn last_token_extraction() {
        // CourtListener opinion-header form: ingest-fetcher stores
        // normalized_name as just the last name. DIME-side must produce
        // the matching last token for the fallback match.
        assert_eq!(dime_name_to_last_token("Tannenwald, Theodore"), "tannenwald");
        assert_eq!(dime_name_to_last_token("O'Connor, Sandra Day"), "o'connor");
        assert_eq!(dime_name_to_last_token("Smith, John D., Jr."), "smith");
        // No comma: still picks the last whitespace token.
        assert_eq!(dime_name_to_last_token("John Smith"), "smith");
    }
}
