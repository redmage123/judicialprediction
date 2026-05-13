//! S5.7 — Layer-2 NLP feature extraction.
//!
//! Two derived signals per opinion, written back to `case_documents`:
//!
//! * **`case_type`** — classified from Code-section references and a few
//!   anchor phrases.  Closed enum mirrored in the CHECK constraint on
//!   `case_documents.case_type`.
//! * **`outcome_for`** — `petitioner` / `respondent` / `split` / `None`.
//!   Detected from "Decision will be entered for ..." phrasings.  `None` when
//!   the opinion ends "under Rule 155" (Rule-155 cases are still in
//!   computation phase — no determination yet) or no disposition is found.
//!
//! A per-judge severity proxy is also computed and merged into `judges.bio`
//! as `{ cases_analyzed: N, wins_for_respondent: M, severity: M/N }` —
//! `severity` here is "fraction of decisions that went against the
//! petitioner", which is the calibration prior the recommender wants.
//!
//! Both regex sets were tuned against the live tax-court corpus on
//! `judicialpredict_postgres` (99 opinions) — see corpus-profile probe in
//! the S5.7 commit message.  Accuracy is enforced by the hand-labelled
//! fixture in `tests/fixtures/labelled_cases.json` (≥ 70% per Sprint 5).

use anyhow::{Context, Result};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Closed case-type vocabulary.  Order matters in `classify_case_type`:
/// more specific signals are checked first, then the residual falls through
/// to `income_tax`.
pub const CASE_TYPES: &[&str] = &[
    "income_tax",
    "innocent_spouse",
    "collection_due_process",
    "whistleblower",
    "estate_tax",
    "gift_tax",
    "partnership",
    "employment_tax",
    "penalty",
];

/// Closed outcome vocabulary.  `None` from `detect_outcome` is also a valid
/// state (unresolved disposition or no clear signal).
pub const OUTCOMES: &[&str] = &["petitioner", "respondent", "split"];

/// Stats from a single extract-features run.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExtractStats {
    pub docs_scanned: u64,
    pub case_type_set: u64,
    pub outcome_set: u64,
    pub judges_updated: u64,
}

// ────────────────────────────────────────────────────────────────────────────
// Pure helpers (testable without a DB)
// ────────────────────────────────────────────────────────────────────────────

/// Classify the case type from the full opinion text.  Always returns a
/// value — defaults to `income_tax` (the most common tax-court case type)
/// when no specific signal is found.
///
/// Priority: specific Code-sections beat generic ones, and case-type-defining
/// signals beat penalty-only signals so that an income-tax case that also
/// imposed a section-6662 penalty still classifies as income_tax.
pub fn classify_case_type(text: &str) -> &'static str {
    let lower = text.to_ascii_lowercase();

    // Most specific Code sections first.
    if lower.contains("section 6015") || lower.contains("innocent spouse") {
        return "innocent_spouse";
    }
    if lower.contains("section 6320")
        || lower.contains("section 6330")
        || lower.contains("collection due process")
        || lower.contains("notice of determination concerning collection")
    {
        return "collection_due_process";
    }
    if lower.contains("section 7623") || lower.contains("whistleblower award") {
        return "whistleblower";
    }
    // Estate / gift — anchored on the dedicated Code chapter so a passing
    // mention of "estate tax" in an income-tax case doesn't trigger.
    if lower.contains("section 2001")
        || lower.contains("section 2031")
        || lower.contains("section 2032")
        || (lower.contains("estate tax") && lower.contains("decedent"))
    {
        return "estate_tax";
    }
    if lower.contains("section 2501")
        || lower.contains("section 2511")
        || (lower.contains("gift tax") && lower.contains("donor"))
    {
        return "gift_tax";
    }
    // Partnership/TEFRA-era proceedings — `section 6221` is the TEFRA anchor;
    // `partnership-level` is the diagnostic phrase that beats the noisy bare
    // "partnership" mention.
    if lower.contains("section 6221")
        || lower.contains("partnership-level")
        || lower.contains("tefra partnership")
    {
        return "partnership";
    }
    // Employment tax — anchored on the Code chapter or "self-employment tax".
    // Bare "FICA" matches in ~88% of the corpus so it's not a useful signal.
    if lower.contains("section 3101")
        || lower.contains("section 3121")
        || lower.contains("self-employment tax")
        || lower.contains("employment tax determination")
    {
        return "employment_tax";
    }
    // Penalty-only — opinion is entirely about an accuracy-related penalty
    // with no income-tax deficiency dispute.  Best signal is "penalty" in
    // the case name combined with a section 6662/6663/6651 reference.
    if (lower.contains("section 6662")
        || lower.contains("section 6663")
        || lower.contains("section 6651"))
        && !lower.contains("deficiency")
    {
        return "penalty";
    }
    "income_tax"
}

/// Detect the disposition outcome.  Returns:
/// * `Some("petitioner")` — clear petitioner win
/// * `Some("respondent")` — clear respondent (IRS) win
/// * `Some("split")` — both phrases appear in the same disposition window
/// * `None` — Rule 155, dismissal-only, or no disposition phrase found
///
/// The disposition window is the text matched by "Decision will be entered…"
/// up to the first period (or 400 chars, whichever comes first) so that
/// later mentions of "petitioner" or "respondent" elsewhere in the opinion
/// don't pollute the signal.
pub fn detect_outcome(text: &str) -> Option<&'static str> {
    // Whitespace-normalize so newline-wrapped phrases match.  We don't
    // lowercase yet because the lowercase pass happens inside the disposition
    // window only.
    let normalized: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();

    // Find the disposition sentence.  The phrase appears 1-3× in tax-court
    // opinions; we scan all occurrences and prefer the most specific one
    // (petitioner+respondent > respondent > petitioner > rule 155).
    let mut best: Option<&'static str> = None;
    for (idx, _) in lower.match_indices("decision will be entered") {
        let window_end = (idx + 400).min(lower.len());
        let window = &lower[idx..window_end];
        // First period closes the sentence.
        let stop = window.find('.').map_or(window.len(), |p| p);
        let window = &window[..stop];

        let has_pet = window.contains("for petitioner");
        let has_resp = window.contains("for respondent");
        let has_rule155 = window.contains("under rule 155") || window.contains("pursuant to rule 155");

        let candidate: Option<&'static str> = match (has_pet, has_resp, has_rule155) {
            (true, true, _) => Some("split"),
            (false, true, _) => Some("respondent"),
            (true, false, _) => Some("petitioner"),
            (false, false, true) => None, // Rule 155 → unresolved
            (false, false, false) => None,
        };

        // "Split" wins over single-party wins; single-party wins over None.
        if let Some(c) = candidate {
            match (best, c) {
                (_, "split") => best = Some("split"),
                (None, x) => best = Some(x),
                (Some("petitioner"), "respondent") | (Some("respondent"), "petitioner") => {
                    best = Some("split")
                }
                _ => {}
            }
        }
    }

    best
}

// ────────────────────────────────────────────────────────────────────────────
// DB pass — populate `case_documents.case_type` / `outcome_for` and roll up
// per-judge severity into `judges.bio`.
// ────────────────────────────────────────────────────────────────────────────

/// Scan `case_documents` for opinions that haven't been classified yet, run
/// [`classify_case_type`] + [`detect_outcome`] on each, and write the results
/// back.  Then roll up per-judge severity into `judges.bio`.
pub async fn run_extraction(pool: &PgPool, tenant_id: Uuid) -> Result<ExtractStats> {
    let mut tx = pool.begin().await.context("begin extract-features tx")?;

    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{tenant_id}'"
    ))
    .execute(&mut *tx)
    .await
    .context("SET LOCAL app.current_tenant_id")?;

    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, court_id, full_text_plain
         FROM case_documents
         WHERE features_extracted_at IS NULL",
    )
    .fetch_all(&mut *tx)
    .await
    .context("select unclassified case_documents")?;

    let mut stats = ExtractStats::default();
    stats.docs_scanned = rows.len() as u64;

    // (court_slug, judge_normalized_name) -> tally
    #[derive(Default)]
    struct JudgeTally {
        analyzed: u64,
        wins_for_respondent: u64,
    }
    let mut tallies: HashMap<(String, String), JudgeTally> = HashMap::new();

    for (doc_id, court_slug, text) in &rows {
        let case_type = classify_case_type(text);
        let outcome = detect_outcome(text);

        sqlx::query(
            "UPDATE case_documents
             SET case_type = $1,
                 outcome_for = $2,
                 features_extracted_at = now()
             WHERE id = $3",
        )
        .bind(case_type)
        .bind(outcome)
        .bind(doc_id)
        .execute(&mut *tx)
        .await
        .context("update case_documents extraction")?;

        stats.case_type_set += 1;
        if outcome.is_some() {
            stats.outcome_set += 1;
        }

        // Roll judges in this opinion into the severity tally.  We reuse the
        // S5.6 extractor (re-imported here to avoid a circular dep) — only
        // judges from the opinion header are counted, so dicta references
        // to other judges don't bias severity.
        for judge_raw in crate::kg::extract_judge_names(text) {
            let normalized = crate::kg::normalize_judge_name(&judge_raw);
            if normalized.is_empty() {
                continue;
            }
            let entry = tallies
                .entry((court_slug.clone(), normalized))
                .or_default();
            entry.analyzed += 1;
            if outcome == Some("respondent") {
                entry.wins_for_respondent += 1;
            }
        }
    }

    // Merge tallies into `judges.bio`.  We look up the judge_id by
    // (tenant_id, normalized_name), and use jsonb_set semantics by simply
    // overwriting the `severity_proxy` key — extraction reruns recompute
    // from scratch.
    for ((_court_slug, normalized), tally) in &tallies {
        let severity = if tally.analyzed == 0 {
            0.0
        } else {
            tally.wins_for_respondent as f64 / tally.analyzed as f64
        };
        let bio_patch = json!({
            "severity_proxy": {
                "cases_analyzed": tally.analyzed,
                "wins_for_respondent": tally.wins_for_respondent,
                "severity": severity,
            }
        });

        let res = sqlx::query(
            "UPDATE judges
             SET bio = bio || $1::jsonb
             WHERE tenant_id = $2 AND normalized_name = $3",
        )
        .bind(&bio_patch)
        .bind(tenant_id)
        .bind(normalized)
        .execute(&mut *tx)
        .await
        .context("update judges.bio severity_proxy")?;

        if res.rows_affected() > 0 {
            stats.judges_updated += 1;
        }
    }

    tx.commit().await.context("commit extract-features tx")?;
    Ok(stats)
}

// ────────────────────────────────────────────────────────────────────────────
// Tests — pure helpers only.  Live-DB coverage is the accuracy fixture in
// tests/extract_accuracy.rs.
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_innocent_spouse() {
        let t = "Petitioner seeks relief under section 6015(f)…";
        assert_eq!(classify_case_type(t), "innocent_spouse");
    }

    #[test]
    fn classifies_cdp() {
        let t = "This is a collection due process case under section 6330.";
        assert_eq!(classify_case_type(t), "collection_due_process");
    }

    #[test]
    fn classifies_whistleblower() {
        let t = "Petitioner appeals the denial of a whistleblower award under section 7623.";
        assert_eq!(classify_case_type(t), "whistleblower");
    }

    #[test]
    fn classifies_estate_tax() {
        let t = "The estate of the decedent reported under section 2001…";
        assert_eq!(classify_case_type(t), "estate_tax");
    }

    #[test]
    fn classifies_partnership() {
        let t = "This TEFRA partnership case under section 6221 involves…";
        assert_eq!(classify_case_type(t), "partnership");
    }

    #[test]
    fn classifies_employment_tax() {
        let t = "Respondent determined self-employment tax under section 1402(a)…";
        assert_eq!(classify_case_type(t), "employment_tax");
    }

    #[test]
    fn classifies_penalty_only_when_no_deficiency() {
        let t = "The sole issue is a section 6662 accuracy-related penalty.";
        assert_eq!(classify_case_type(t), "penalty");
    }

    #[test]
    fn penalty_with_deficiency_is_income_tax() {
        let t = "Respondent determined a deficiency in income tax and a section 6662 penalty.";
        assert_eq!(classify_case_type(t), "income_tax");
    }

    #[test]
    fn default_is_income_tax() {
        let t = "Respondent issued a notice of deficiency for tax year 2018.";
        assert_eq!(classify_case_type(t), "income_tax");
    }

    #[test]
    fn outcome_respondent() {
        let t = "After consideration of the record…\n\nDecision will be entered for respondent.";
        assert_eq!(detect_outcome(t), Some("respondent"));
    }

    #[test]
    fn outcome_petitioner() {
        let t = "We hold for petitioner.\n\nDecision will be entered for petitioner.";
        assert_eq!(detect_outcome(t), Some("petitioner"));
    }

    #[test]
    fn outcome_split_within_one_sentence() {
        let t = "Decision will be entered for respondent as to the deficiency and for \
                 petitioners as to the accuracy-related penalty.";
        assert_eq!(detect_outcome(t), Some("split"));
    }

    #[test]
    fn outcome_rule_155_is_unresolved() {
        let t = "Decision will be entered under Rule 155.";
        assert_eq!(detect_outcome(t), None);
    }

    #[test]
    fn outcome_handles_newline_wrap() {
        // Real corpus: "Decision will be entered for\n\n      respondent."
        let t = "Decision will be entered for\n\n      respondent.";
        assert_eq!(detect_outcome(t), Some("respondent"));
    }

    #[test]
    fn outcome_none_when_no_phrase() {
        let t = "The petition is dismissed for lack of jurisdiction.";
        assert_eq!(detect_outcome(t), None);
    }

    #[test]
    fn outcome_split_across_two_separate_decisions() {
        // Some opinions have multiple "Decision will be entered" sentences,
        // one petitioner and one respondent — that's a split too.
        let t = "Decision will be entered for petitioner.\n\nDecision will be entered \
                 for respondent.";
        assert_eq!(detect_outcome(t), Some("split"));
    }
}
