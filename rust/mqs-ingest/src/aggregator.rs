//! Group MqsRow records by `justice_id`, sort the per-justice rows by
//! `term`, and surface the latest-term row's score so the gateway's
//! hot-path JSONB lookup is O(1).
//!
//! Defensive on data quality:
//!   * Rows with `post_mean = None` are skipped at this stage — they
//!     can't drive a `latest_score`. They still go into `scores[]` as
//!     `null` so the audit trail is complete.
//!   * If two rows share the same `(justice_id, term)` pair (shouldn't
//!     happen in the real release but defensive), we keep the later
//!     one and warn.

use std::collections::BTreeMap;

use crate::parser::MqsRow;

/// One justice's entire series, sorted ascending by term.
#[derive(Debug, Clone)]
pub struct AggregatedJustice {
    pub justice_id: String,
    pub name: String,
    pub scores: Vec<TermScore>,
    /// Highest-term row with a non-null post_mean. May be None if
    /// every row in this justice's series has a NULL post_mean.
    pub latest_score: Option<f64>,
    pub latest_term: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct TermScore {
    pub term: i32,
    pub post_mean: Option<f64>,
    pub post_sd: Option<f64>,
}

pub fn aggregate_by_justice(rows: Vec<MqsRow>) -> Vec<AggregatedJustice> {
    // BTreeMap so the output is deterministic for snapshot tests.
    let mut groups: BTreeMap<String, (String, BTreeMap<i32, TermScore>)> = BTreeMap::new();

    for row in rows {
        if row.justice_id.trim().is_empty() {
            continue;
        }
        let entry = groups
            .entry(row.justice_id.clone())
            .or_insert_with(|| (row.justice_name.clone(), BTreeMap::new()));

        // BTreeMap on term — duplicate terms get the later-arriving row
        // (with a warn).
        let was_present = entry.1.insert(
            row.term,
            TermScore {
                term: row.term,
                post_mean: row.post_mean,
                post_sd: row.post_sd,
            },
        );
        if was_present.is_some() {
            tracing::warn!(
                justice_id = %row.justice_id,
                term = row.term,
                "duplicate (justice, term) pair; keeping the later row"
            );
        }
    }

    let mut out = Vec::new();
    for (justice_id, (name, terms)) in groups {
        let scores: Vec<TermScore> = terms.into_values().collect();
        // Find the highest-term row with a non-null post_mean. Walk in
        // reverse so we get latest first.
        let (latest_term, latest_score) = scores
            .iter()
            .rev()
            .find_map(|s| s.post_mean.map(|pm| (Some(s.term), Some(pm))))
            .unwrap_or((None, None));
        out.push(AggregatedJustice {
            justice_id,
            name,
            scores,
            latest_score,
            latest_term,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::MqsRow;

    fn row(jid: &str, name: &str, term: i32, post_mean: Option<f64>) -> MqsRow {
        MqsRow {
            justice_name: name.to_string(),
            term,
            post_mean,
            post_sd: None,
            justice_id: jid.to_string(),
        }
    }

    #[test]
    fn groups_and_picks_latest() {
        let rows = vec![
            row("j-001", "Marshall, Thurgood", 1969, Some(-1.2)),
            row("j-001", "Marshall, Thurgood", 1970, Some(-1.43)),
            row("j-001", "Marshall, Thurgood", 1968, Some(-1.0)),
            row("j-002", "Roberts, John G.",  2010, Some(0.72)),
        ];
        let agg = aggregate_by_justice(rows);
        assert_eq!(agg.len(), 2);

        let marshall = agg.iter().find(|a| a.justice_id == "j-001").unwrap();
        assert_eq!(marshall.scores.len(), 3);
        // Sorted ascending by term.
        assert_eq!(marshall.scores[0].term, 1968);
        assert_eq!(marshall.scores[2].term, 1970);
        // Latest term + score = highest-term non-null row.
        assert_eq!(marshall.latest_term, Some(1970));
        assert_eq!(marshall.latest_score, Some(-1.43));
    }

    #[test]
    fn skips_null_post_mean_for_latest() {
        let rows = vec![
            row("j-001", "Marshall, T.", 1969, Some(-1.2)),
            row("j-001", "Marshall, T.", 1970, None), // newer but null
        ];
        let agg = aggregate_by_justice(rows);
        let m = &agg[0];
        // latest_term should be the highest term WITH a value, not the
        // highest term overall.
        assert_eq!(m.latest_term, Some(1969));
        assert_eq!(m.latest_score, Some(-1.2));
        // But the null row is still in scores[] for audit.
        assert_eq!(m.scores.len(), 2);
    }

    #[test]
    fn missing_justice_id_dropped() {
        let rows = vec![row("", "Anon, A.", 1990, Some(0.1))];
        let agg = aggregate_by_justice(rows);
        assert!(agg.is_empty());
    }
}
