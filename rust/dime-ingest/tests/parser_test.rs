//! Integration test for the CSV parser. The unit tests in parser.rs cover
//! the name normaliser; this one covers the file-level shape: header
//! presence, NULL handling, malformed-row tolerance.

use dime_ingest::parser::parse_dime_csv;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("fixtures");
    p.push(name);
    p
}

#[test]
fn parses_the_mini_fixture() {
    let rows = parse_dime_csv(&fixture("dime-judges-mini.csv")).expect("parse");
    // 25 rows in the file.
    assert_eq!(rows.len(), 25);
}

#[test]
fn empty_cfscore_becomes_none() {
    let rows = parse_dime_csv(&fixture("dime-judges-mini.csv")).expect("parse");
    let kawashima = rows.iter().find(|r| r.bonica_id == "dime-007").unwrap();
    assert!(kawashima.cfscore.is_none(), "empty cfscore should be None");
}

#[test]
fn na_cfscore_becomes_none() {
    let rows = parse_dime_csv(&fixture("dime-judges-mini.csv")).expect("parse");
    let quincy = rows.iter().find(|r| r.bonica_id == "dime-025").unwrap();
    assert!(quincy.cfscore.is_none(), "NA cfscore should be None");
}

#[test]
fn numeric_cfscore_round_trips() {
    let rows = parse_dime_csv(&fixture("dime-judges-mini.csv")).expect("parse");
    let tannenwald = rows.iter().find(|r| r.bonica_id == "dime-001").unwrap();
    assert_eq!(tannenwald.cfscore, Some(-0.41));
}

#[test]
fn duplicate_name_different_court_is_two_rows() {
    let rows = parse_dime_csv(&fixture("dime-judges-mini.csv")).expect("parse");
    let smiths: Vec<_> = rows.iter().filter(|r| r.name.starts_with("Smith, John A")).collect();
    assert_eq!(smiths.len(), 2, "Smith, John A. on two courts should produce two rows");
    let courts: Vec<&str> = smiths.iter().map(|r| r.court.as_str()).collect();
    assert!(courts.contains(&"scotus"));
    assert!(courts.contains(&"cafc"));
}

#[test]
fn empty_court_is_kept_as_empty_string() {
    let rows = parse_dime_csv(&fixture("dime-judges-mini.csv")).expect("parse");
    let smith16 = rows.iter().find(|r| r.bonica_id == "dime-016").unwrap();
    assert_eq!(smith16.court, "");
}
