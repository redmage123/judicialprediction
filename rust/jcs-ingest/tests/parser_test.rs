//! Integration test for the JCS CSV parser.

use jcs_ingest::parser::parse_jcs_csv;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("fixtures");
    p.push(name);
    p
}

#[test]
fn parses_mini_fixture() {
    let rows = parse_jcs_csv(&fixture("jcs-mini.csv")).expect("parse");
    assert_eq!(rows.len(), 25);
}

#[test]
fn empty_jcs_becomes_none() {
    let rows = parse_jcs_csv(&fixture("jcs-mini.csv")).expect("parse");
    let estrada = rows.iter().find(|r| r.judge_id == "emqs-estrada").unwrap();
    assert!(estrada.jcs.is_none(), "empty jcs should be None");
}

#[test]
fn na_jcs_becomes_none() {
    let rows = parse_jcs_csv(&fixture("jcs-mini.csv")).expect("parse");
    let anon = rows.iter().find(|r| r.judge_id == "emqs-anon").unwrap();
    assert!(anon.jcs.is_none(), "NA jcs should be None");
}

#[test]
fn numeric_jcs_round_trips() {
    let rows = parse_jcs_csv(&fixture("jcs-mini.csv")).expect("parse");
    let marshall = rows.iter().find(|r| r.judge_id == "emqs-marshall").unwrap();
    assert_eq!(marshall.jcs, Some(-0.73));
    assert_eq!(marshall.court, "scotus");
}

#[test]
fn court_kept_verbatim() {
    let rows = parse_jcs_csv(&fixture("jcs-mini.csv")).expect("parse");
    let posner = rows.iter().find(|r| r.judge_id == "emqs-posner").unwrap();
    assert_eq!(posner.court, "ca7");
}
