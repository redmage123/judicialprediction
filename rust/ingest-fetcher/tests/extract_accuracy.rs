//! S5.7 — accuracy gate for the Layer-2 NLP feature extractor.
//!
//! Sprint 5 risk plan calls for "≥ 70% accuracy on a hand-labelled fixture
//! before the extractor is allowed to drive `createCase` defaults" (S5.8).
//! This test enforces that bar.  The fixture is intentionally small (~20
//! examples) and covers every branch in `classify_case_type` /
//! `detect_outcome`; once the live corpus grows beyond ~1,000 opinions the
//! fixture should be replaced with a stratified sample (Sprint 6+).
//!
//! Outcome is treated as 4-class: `petitioner` / `respondent` / `split` /
//! `null`, with `null` meaning "unresolved disposition" — the extractor must
//! return `None` for Rule-155 and dismissal-only opinions.

use ingest_fetcher::{classify_case_type, detect_outcome};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LabelledCase {
    id: String,
    case_type: String,
    /// Owned `Option<String>`; the extractor returns `&'static str`.  Compare
    /// via `as_deref`.
    #[serde(default)]
    outcome_for: Option<String>,
    text: String,
}

fn load_fixture() -> Vec<LabelledCase> {
    let raw = include_str!("fixtures/labelled_cases.json");
    serde_json::from_str(raw).expect("labelled_cases.json must parse")
}

#[test]
fn classify_case_type_accuracy() {
    let cases = load_fixture();
    let total = cases.len();
    let mut correct = 0usize;
    let mut wrong: Vec<String> = Vec::new();

    for c in &cases {
        let predicted = classify_case_type(&c.text);
        if predicted == c.case_type {
            correct += 1;
        } else {
            wrong.push(format!("{}: expected {} got {}", c.id, c.case_type, predicted));
        }
    }

    let pct = (correct as f64 / total as f64) * 100.0;
    assert!(
        pct >= 70.0,
        "case_type accuracy {:.1}% (< 70% gate); misses:\n{}",
        pct,
        wrong.join("\n"),
    );
}

#[test]
fn detect_outcome_accuracy() {
    let cases = load_fixture();
    let total = cases.len();
    let mut correct = 0usize;
    let mut wrong: Vec<String> = Vec::new();

    for c in &cases {
        let predicted: Option<&str> = detect_outcome(&c.text);
        let expected: Option<&str> = c.outcome_for.as_deref();
        if predicted == expected {
            correct += 1;
        } else {
            wrong.push(format!(
                "{}: expected {:?} got {:?}",
                c.id, expected, predicted
            ));
        }
    }

    let pct = (correct as f64 / total as f64) * 100.0;
    assert!(
        pct >= 70.0,
        "outcome accuracy {:.1}% (< 70% gate); misses:\n{}",
        pct,
        wrong.join("\n"),
    );
}
