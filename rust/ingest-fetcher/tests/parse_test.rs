//! Fixture-based tests for the parser. The fixture tarball contains 8
//! entries: 6 well-formed, 1 with optional fields missing, 1 with corrupt
//! JSON. Total expected: 7 Ok, 1 Err.

use std::fs::File;
use std::path::PathBuf;

use ingest_fetcher::parse_tarball;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.tar.gz")
}

#[test]
fn fixture_yields_seven_valid_one_err() {
    let f = File::open(fixture_path()).expect("open fixture");
    let results = parse_tarball(f);
    assert_eq!(results.len(), 8, "8 entries surface");

    let ok_count = results.iter().filter(|r| r.is_ok()).count();
    let err_count = results.iter().filter(|r| r.is_err()).count();
    assert_eq!(ok_count, 7, "7 valid opinions");
    assert_eq!(err_count, 1, "1 corrupt entry surfaced as Err");
}

#[test]
fn fixture_partial_entry_uses_defaults() {
    let f = File::open(fixture_path()).expect("open fixture");
    let results = parse_tarball(f);
    let partial = results
        .into_iter()
        .filter_map(|r| r.ok())
        .find(|op| op.opinion_id == 1007)
        .expect("partial fixture entry parsed");
    assert_eq!(partial.case_name, None);
    assert_eq!(partial.date_filed, None);
    assert_eq!(partial.source_url, None);
    assert_eq!(partial.citation_count, 0);
    assert_eq!(partial.court_id, "tax");
}

#[test]
fn fixture_full_entry_round_trips_all_fields() {
    let f = File::open(fixture_path()).expect("open fixture");
    let results = parse_tarball(f);
    let full = results
        .into_iter()
        .filter_map(|r| r.ok())
        .find(|op| op.opinion_id == 1006)
        .expect("opinion 1006 parsed");
    assert_eq!(full.court_id, "tax");
    assert_eq!(full.case_name.as_deref(), Some("Wilson Holdings, LLC v. Commissioner"));
    assert_eq!(full.citation_count, 12);
    assert!(full.full_text_plain.contains("section 754 election"));
    assert!(full.source_url.unwrap().contains("/opinion/1006/"));
    assert_eq!(
        full.date_filed,
        Some(chrono::NaiveDate::from_ymd_opt(2024, 7, 30).unwrap())
    );
}
