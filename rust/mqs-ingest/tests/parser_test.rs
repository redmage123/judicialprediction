//! Integration test for the MQ CSV parser + aggregator over the fixture.

use mqs_ingest::aggregator::aggregate_by_justice;
use mqs_ingest::parser::parse_mqs_csv;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("fixtures");
    p.push(name);
    p
}

#[test]
fn parses_mini_fixture() {
    let rows = parse_mqs_csv(&fixture("mqs-mini.csv")).expect("parse");
    // 25 data rows in the fixture.
    assert_eq!(rows.len(), 25);
}

#[test]
fn na_post_mean_becomes_none() {
    let rows = parse_mqs_csv(&fixture("mqs-mini.csv")).expect("parse");
    let null_row = rows
        .iter()
        .find(|r| r.justice_name.starts_with("Ginsburg") && r.term == 2019)
        .unwrap();
    assert!(null_row.post_mean.is_none());
}

#[test]
fn aggregator_picks_latest_valid_term() {
    let rows = parse_mqs_csv(&fixture("mqs-mini.csv")).expect("parse");
    let agg = aggregate_by_justice(rows);

    let ginsburg = agg.iter().find(|a| a.justice_id == "mq-ginsburg").unwrap();
    // 2019 has NA post_mean; latest valid is 2018.
    assert_eq!(ginsburg.latest_term, Some(2018));
    assert_eq!(ginsburg.latest_score, Some(-1.15));
}

#[test]
fn aggregator_drops_missing_justice_id() {
    let rows = parse_mqs_csv(&fixture("mqs-mini.csv")).expect("parse");
    let agg = aggregate_by_justice(rows);
    // "Anon, Anonymous" has empty justiceID and should be dropped.
    assert!(agg.iter().all(|a| !a.name.contains("Anon")));
}

#[test]
fn aggregator_collapses_duplicate_terms() {
    let rows = parse_mqs_csv(&fixture("mqs-mini.csv")).expect("parse");
    let agg = aggregate_by_justice(rows);
    let dup = agg.iter().find(|a| a.justice_id == "mq-dup").unwrap();
    // Both rows had term=1990; should collapse to one entry.
    assert_eq!(dup.scores.len(), 1);
    // The later (post_mean=0.2) wins per the BTreeMap-replace contract.
    assert_eq!(dup.scores[0].post_mean, Some(0.2));
}

#[test]
fn marshall_full_series_loaded() {
    let rows = parse_mqs_csv(&fixture("mqs-mini.csv")).expect("parse");
    let agg = aggregate_by_justice(rows);
    let m = agg.iter().find(|a| a.justice_id == "mq-marshall").unwrap();
    // 6 terms in the fixture.
    assert_eq!(m.scores.len(), 6);
    // Latest = 1972.
    assert_eq!(m.latest_term, Some(1972));
    assert_eq!(m.latest_score, Some(-1.55));
}
