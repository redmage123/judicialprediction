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
/// * `Some("split")` — mixed disposition
/// * `None` — Rule 155, dismissal-only, or no disposition phrase found
///
/// Tax-court (and any court that uses the "decision will be entered" idiom)
/// is handled by [`detect_outcome_taxcourt`]; federal-circuit appellate
/// dispositions (`AFFIRMED`/`REVERSED`/`VACATED` blocks) are handled by
/// [`detect_outcome_appellate`].  Use [`detect_outcome_for_court`] when the
/// court_id is known — it picks the right scanner.  The bare entry point
/// still tries both in fallback order so older call sites and tests don't
/// regress.
pub fn detect_outcome(text: &str) -> Option<&'static str> {
    detect_outcome_taxcourt(text).or_else(|| detect_outcome_appellate(text))
}

/// Court family used to route opinion text to the right disposition scanner.
/// The mapping from a court_id slug to a family is intentionally permissive on
/// the prefix side (district-court slugs are open-ended — we can't enumerate
/// all 94) but conservative on the disposition side (the scanners themselves
/// prefer None over a wrong guess).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CourtFamily {
    /// Supreme Court of the United States.  CourtListener uses `scotus`;
    /// CAP's reporter-based slug is `us`.
    Scotus,
    /// Federal courts of appeals.  CourtListener uses `cafc`, `ca1`-`ca11`,
    /// and `cadc`; CAP's reporter-based slugs are `f1d`, `f2d`, `f3d`, `f4th`.
    Appellate,
    /// Federal district courts.  CourtListener uses slugs like `nyd`, `cad`,
    /// `txnd`, `dnj` — the convention is that the slug ends in `d` and is not
    /// one of the well-known appellate or supreme-court slugs.
    District,
    /// US Tax Court — uses the "decision will be entered" idiom.
    Tax,
    /// Federal bankruptcy courts.  Disposition convention differs from the
    /// district-court Rule-56 form; deferred to Sprint 16.
    Bankruptcy,
    /// Everything else — falls back to the generic [`detect_outcome`].
    Unknown,
}

/// Classify a court_id slug into a [`CourtFamily`].  Recognizes CourtListener,
/// CAP reporter-based, and govinfo slug conventions.  Unknown slugs return
/// [`CourtFamily::Unknown`] so the generic fallback applies.
pub fn court_family(court_id: &str) -> CourtFamily {
    let id = court_id.to_ascii_lowercase();

    // SCOTUS — both CourtListener and CAP slugs.
    if id == "scotus" || id == "us" {
        return CourtFamily::Scotus;
    }

    // Tax court — CourtListener uses `tax`; DAWSON uses `tax-court`.
    if id == "tax" || id == "tax-court" || id == "ustc" {
        return CourtFamily::Tax;
    }

    // Bankruptcy — CourtListener uses `bankr.<district>` / `bankr_<district>`
    // family slugs; we deliberately don't try to recover anything from them
    // in Sprint 15.
    if id.starts_with("bankr") {
        return CourtFamily::Bankruptcy;
    }

    // Appellate — federal-circuit, numbered circuits, DC circuit, and CAP's
    // reporter-based slugs (`f1d`, `f2d`, `f3d`, `f4th`).
    if id == "cafc" || id == "cadc" || id == "f1d" || id == "f2d" || id == "f3d" || id == "f4th" {
        return CourtFamily::Appellate;
    }
    if let Some(rest) = id.strip_prefix("ca") {
        // ca1..ca11
        if rest.parse::<u32>().map(|n| (1..=11).contains(&n)).unwrap_or(false) {
            return CourtFamily::Appellate;
        }
    }

    // District — CourtListener convention is one of:
    //   * `d<state>` for single-district states (`dnj`, `dmd`, `dde`, `dnh`,
    //     `dor`, `dvt`, `dme`, `dri`, ...).  3-4 chars starting with `d`.
    //   * `<state><division>` with division suffix `cd` / `md` / `nd` / `sd`
    //     / `ed` / `wd` (`nysd`, `nyed`, `cacd`, `txnd`, `flsd`, ...).
    //   * Govinfo / CAP variants: `usdc<...>`, slug containing `_d_`, slug
    //     ending in `dist`.
    //
    // Intentionally over-broad on the recognition side; the scanner itself
    // prefers None on ambiguous text.
    let division_suffixes = ["cd", "md", "nd", "sd", "ed", "wd"];
    let ends_in_division = division_suffixes.iter().any(|suf| id.ends_with(suf));
    if ends_in_division && id.len() >= 3 {
        return CourtFamily::District;
    }
    // Single-district `d<state>` pattern: 3-4 chars, starts with `d`, and the
    // tail is 2-3 alphabetic chars (e.g. `dnj`, `dmd`, `dde`).  Excludes `dc`
    // (DC Circuit appellate, already claimed above) and ensures the tail is
    // alphabetic so we don't accept stray numeric IDs.
    if id.starts_with('d') && (id.len() == 3 || id.len() == 4) && id != "dc" {
        let rest = &id[1..];
        if rest.chars().all(|c| c.is_ascii_alphabetic()) {
            return CourtFamily::District;
        }
    }
    // Short reporter-style district slugs (`nyd`, `cad`, ...) — 3 chars
    // ending in `d`.  These are not standard CourtListener slugs but appear
    // in legacy fixtures and reporter abbreviations.
    if id.len() == 3 && id.ends_with('d') && !id.starts_with('d') {
        let head = &id[..2];
        if head.chars().all(|c| c.is_ascii_alphabetic()) {
            return CourtFamily::District;
        }
    }
    if id.starts_with("usdc") || id.contains("_d_") || id.ends_with("dist") {
        return CourtFamily::District;
    }

    CourtFamily::Unknown
}

/// Same as [`detect_outcome`] but dispatches by court_id via [`court_family`].
/// Routing keeps appellate-style affirmance language (which appears in dicta
/// about prior appeals in tax-court opinions, and vice-versa) from leaking
/// into the outcome.
///
/// * SCOTUS → [`detect_outcome_scotus`]
/// * Appellate (CAFC, CA1-CA11, CADC, F1d-F4th) → [`detect_outcome_appellate`]
/// * District → [`detect_outcome_district`]
/// * Tax → [`detect_outcome_taxcourt`]
/// * Bankruptcy → `None` (Sprint 16)
/// * Unknown → generic [`detect_outcome`] fallback
pub fn detect_outcome_for_court(court_id: &str, text: &str) -> Option<&'static str> {
    match court_family(court_id) {
        CourtFamily::Scotus => detect_outcome_scotus(text),
        CourtFamily::Appellate => detect_outcome_appellate(text),
        CourtFamily::District => detect_outcome_district(text),
        CourtFamily::Tax => detect_outcome_taxcourt(text),
        CourtFamily::Bankruptcy => None,
        CourtFamily::Unknown => detect_outcome(text),
    }
}

/// Tax-court disposition scanner.  Matches both singular ("Decision will be
/// entered") and plural ("Decisions will be entered") forms — the plural form
/// is common when a single opinion resolves several taxable years.
///
/// The disposition window is the text matched by the phrase up to the first
/// period (or 400 chars, whichever comes first) so that later mentions of
/// "petitioner" or "respondent" elsewhere in the opinion don't pollute the
/// signal.
fn detect_outcome_taxcourt(text: &str) -> Option<&'static str> {
    // Whitespace-normalize so newline-wrapped phrases match.  We don't
    // lowercase yet because the lowercase pass happens inside the disposition
    // window only.
    let normalized: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();

    let mut best: Option<&'static str> = None;
    // Scan both "decision will be entered" and "decisions will be entered".
    // The plural form is the only added case; everything else is unchanged.
    for anchor in ["decision will be entered", "decisions will be entered"] {
        for (idx, _) in lower.match_indices(anchor) {
            let window_end = (idx + 400).min(lower.len());
            let window = &lower[idx..window_end];
            let stop = window.find('.').map_or(window.len(), |p| p);
            let window = &window[..stop];

            let has_pet = window.contains("for petitioner");
            let has_resp = window.contains("for respondent");
            let has_rule155 = window.contains("under rule 155") || window.contains("pursuant to rule 155");

            let candidate: Option<&'static str> = match (has_pet, has_resp, has_rule155) {
                (true, true, _) => Some("split"),
                (false, true, _) => Some("respondent"),
                (true, false, _) => Some("petitioner"),
                (false, false, true) => None,
                (false, false, false) => None,
            };

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
    }

    best
}

/// Federal-circuit appellate disposition scanner.  Federal-Circuit opinions
/// (and similarly-styled appellate decisions) end with an all-caps disposition
/// block — typically the last 600 characters of the opinion.  We look for the
/// keywords in that tail only, so dicta references to prior appellate history
/// in the body don't pollute the signal.
///
/// Convention: the appellant is the party challenging the lower-court ruling,
/// i.e. the **petitioner** in our binary.  So:
///   * `AFFIRMED` (lower-court stands) → respondent wins
///   * `REVERSED` (lower-court overturned) → petitioner wins
///   * `VACATED` (lower-court set aside) → petitioner wins (boundary-favorable;
///     vacatur is petitioner-favorable as a binary outcome even though it
///     usually triggers remand)
///   * Mixed forms ("AFFIRMED-IN-PART, ..., REVERSED-IN-PART", "AFFIRMED IN
///     PART AND REVERSED IN PART", etc.) → split
///   * `DISMISSED` → None (jurisdictional, not on the merits)
fn detect_outcome_appellate(text: &str) -> Option<&'static str> {
    // Window the LAST 600 chars — CAFC dispositions live in the closing
    // block.  If the opinion is shorter, scan the whole text.
    let tail_start = text.len().saturating_sub(600);
    let tail = &text[tail_start..];
    // Whitespace-normalize the tail so multi-line dispositions like
    // "AFFIRMED-IN-PART, VACATED-IN-PART,\n             REVERSED-IN-PART."
    // collapse to a single line we can scan.
    let normalized: String = tail.split_whitespace().collect::<Vec<_>>().join(" ");
    let upper = normalized.to_ascii_uppercase();

    // Dismissal short-circuits — it's a jurisdictional disposition, not a
    // merits outcome, so we surface as None rather than guessing a winner.
    if upper.contains("DISMISSED") && !upper.contains("AFFIRMED") && !upper.contains("REVERSED") {
        return None;
    }

    // "IN PART" markers indicate a split disposition regardless of which
    // verbs surround them.
    let in_part = upper.contains("IN-PART") || upper.contains("IN PART");
    let has_affirmed = upper.contains("AFFIRMED");
    let has_reversed = upper.contains("REVERSED");
    let has_vacated = upper.contains("VACATED");

    // Mixed-disposition forms.
    if in_part && (has_affirmed as u8 + has_reversed as u8 + has_vacated as u8) >= 2 {
        return Some("split");
    }
    // Both AFFIRMED + REVERSED without "IN PART" still indicates a mixed
    // disposition (some CAFC orders just say "AFFIRMED. REVERSED." on
    // separate counts).
    if has_affirmed && (has_reversed || has_vacated) {
        return Some("split");
    }

    if has_reversed || has_vacated {
        return Some("petitioner");
    }
    if has_affirmed {
        return Some("respondent");
    }

    None
}

/// SCOTUS disposition scanner.  Unlike federal-circuit opinions, SCOTUS
/// opinions don't end with all-caps disposition blocks — the disposition is
/// stated in prose ("The judgment of the Court of Appeals is affirmed.", etc.)
/// at the very end of the opinion.  We scan a larger window than the appellate
/// scanner (~800 chars) because SCOTUS opinions sometimes carry a syllabus or
/// per-curiam preamble that pushes the operative phrase further from the end.
///
/// Convention: in our binary, the **petitioner** is the party that brought the
/// cert petition (typically the loser below).  So:
///   * `affirmed` near "judgment"/"decision" → respondent wins (judgment below
///     stands)
///   * `reversed` near "judgment"/"decision" → petitioner wins
///   * `vacated` near "judgment"/"decision" → petitioner wins
///   * mixed forms ("affirmed in part, reversed in part") → split
///   * DIG ("dismissed as improvidently granted") → None (procedural)
///
/// **Conservative**: we require the disposition verb to co-occur with
/// "judgment" or "decision" or be preceded by "is" so that body dicta like
/// "The Court of Appeals affirmed in 2018" doesn't pollute the signal.  Better
/// to skip an ambiguous case than mislabel it — S15.9 calibrates precision
/// against SCDB labels.
fn detect_outcome_scotus(text: &str) -> Option<&'static str> {
    // Window the LAST 800 chars — SCOTUS dispositions can be a paragraph
    // longer than CAFC blocks because they're written in prose, not all-caps.
    let tail_start = text.len().saturating_sub(800);
    let tail = &text[tail_start..];
    let normalized: String = tail.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();

    // DIG short-circuits — procedural, not on the merits.
    if lower.contains("dismissed as improvidently granted") {
        return None;
    }

    // "In part" markers indicate a mixed disposition.  When the "in part"
    // idiom is present we relax the strict phrase-match requirement and
    // accept any two of the disposition verbs as a split, because the
    // canonical SCOTUS form is "the judgment is affirmed in part and
    // reversed in part" — the second verb won't satisfy the "is <verb>"
    // pattern even though it's clearly part of the disposition.
    let in_part = lower.contains("in part");
    let has_affirmed = scotus_phrase_match(&lower, "affirmed");
    let has_reversed = scotus_phrase_match(&lower, "reversed");
    let has_vacated = scotus_phrase_match(&lower, "vacated");

    if in_part {
        let verb_count = ["affirmed", "reversed", "vacated"]
            .iter()
            .filter(|v| lower.contains(*v))
            .count();
        if verb_count >= 2 {
            return Some("split");
        }
    }
    // Bare "Reversed and remanded." / "Vacated and remanded." at the end is a
    // very common SCOTUS short-form disposition.
    if has_affirmed && (has_reversed || has_vacated) {
        return Some("split");
    }

    if has_reversed || has_vacated {
        return Some("petitioner");
    }
    if has_affirmed {
        return Some("respondent");
    }

    None
}

/// Helper for [`detect_outcome_scotus`].  A SCOTUS disposition verb counts
/// only when it appears as the legal-disposition idiom — either:
///   * immediately preceded by " is " (as in "the judgment ... is affirmed"),
///   * as the leading word of a short-form sentence (e.g. "Reversed and
///     remanded."), or
///   * within ~30 chars of "judgment" or "decision".
///
/// Body dicta like "the Court of Appeals affirmed in 2018" are excluded.
fn scotus_phrase_match(lower: &str, verb: &str) -> bool {
    // Pattern A: "is <verb>" (the most common form: "...the judgment is
    // affirmed.").
    let is_pat = format!(" is {verb}");
    if lower.contains(&is_pat) {
        return true;
    }
    // Pattern B: short-form sentence — verb appears at the start of the tail
    // or after a period (e.g. "Reversed and remanded.").
    if lower.starts_with(&format!("{verb} ")) || lower.starts_with(&format!("{verb}.")) {
        return true;
    }
    let dot_pat = format!(". {verb}");
    if lower.contains(&dot_pat) {
        return true;
    }
    // Pattern C: verb within 30 chars of "judgment" / "decision".  Bounded
    // window so far-away mentions don't co-trigger.
    for anchor in ["judgment", "decision"] {
        for (a_idx, _) in lower.match_indices(anchor) {
            // Look in a 60-char window around the anchor for the verb.
            let lo = a_idx.saturating_sub(30);
            let hi = (a_idx + anchor.len() + 30).min(lower.len());
            if lower[lo..hi].contains(verb) {
                // But still exclude bare "[Court] affirmed in <year>"
                // history-style dicta — i.e. require an "is" or "are"
                // somewhere in the window so we're seeing the legal idiom,
                // not the past-tense narrative.
                let win = &lower[lo..hi];
                if win.contains(" is ") || win.contains(" are ") {
                    return true;
                }
            }
        }
    }
    false
}

/// Federal district-court disposition scanner.  District opinions don't use
/// the all-caps appellate blocks; they end with verbose Rule 12 / Rule 56 /
/// Rule 41 orderings ("DENIED", "GRANTED IN PART", "DISMISSED").
///
/// The plaintiff/defendant vocabulary doesn't map cleanly to our
/// petitioner/respondent binary (it depends on case posture), so this scanner
/// is **deliberately conservative**:
///   * "motion to dismiss ... granted" or "complaint is dismissed" →
///     respondent wins (the defending side wins a threshold motion)
///   * "plaintiff's motion ... granted" (no qualifier) → petitioner wins
///   * "defendant's motion ... granted" (no qualifier) → respondent wins
///   * any "in part" qualifier with both grant + deny → split
///   * everything else → None
///
/// Recall will be low — that's intentional.  Mislabeling district-court
/// outcomes is worse for the downstream model than skipping them.
fn detect_outcome_district(text: &str) -> Option<&'static str> {
    // Window the last 1000 chars — district orderings are wordy.
    let tail_start = text.len().saturating_sub(1000);
    let tail = &text[tail_start..];
    let normalized: String = tail.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();

    // "In part" + both grant and deny → split.
    let in_part = lower.contains("in part");
    let has_granted = lower.contains("granted");
    let has_denied = lower.contains("denied");
    if in_part && has_granted && has_denied {
        return Some("split");
    }

    // Motion-to-dismiss granted → respondent.  We also catch the closing
    // "complaint is dismissed (with|without) prejudice" idiom which is the
    // most reliable district-court signal.
    let dismiss_granted = lower.contains("motion to dismiss is granted")
        || lower.contains("motion to dismiss") && lower.contains("granted")
            && !lower.contains("motion to dismiss is denied");
    let complaint_dismissed = lower.contains("complaint is dismissed")
        || lower.contains("action is dismissed")
        || lower.contains("case is dismissed");
    if dismiss_granted || complaint_dismissed {
        return Some("respondent");
    }

    // Plaintiff's vs defendant's motion granted.  Use simple substring checks
    // — we accept some false negatives in exchange for keeping precision
    // high.
    let pl_granted = lower.contains("plaintiff's motion") && lower.contains("granted")
        && !lower.contains("plaintiff's motion is denied")
        && !lower.contains("plaintiff's motion for summary judgment is denied");
    let def_granted = lower.contains("defendant's motion") && lower.contains("granted")
        && !lower.contains("defendant's motion is denied")
        && !lower.contains("defendant's motion for summary judgment is denied");

    match (pl_granted, def_granted) {
        (true, true) => Some("split"),
        (true, false) => Some("petitioner"),
        (false, true) => Some("respondent"),
        (false, false) => None,
    }
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
        let outcome = detect_outcome_for_court(court_slug, text);

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

    // ── New plural-form + CAFC dispositions (Sprint 14) ────────────────────

    #[test]
    fn outcome_plural_decisions_respondent() {
        // Real corpus: "Decisions will be entered for respondent for the
        // taxable years 2006 and 2007 and under Rule 155 for the taxable
        // year 2008".  Plural form was missed before Sprint 14.
        let t = "Decisions will be entered for respondent for the taxable years 2006 \
                 and 2007 and under Rule 155 for the taxable year 2008";
        assert_eq!(detect_outcome(t), Some("respondent"));
    }

    #[test]
    fn outcome_appellate_affirmed_is_respondent() {
        // Federal-circuit appeal where the lower-court ruling stands.
        let t = "For the foregoing reasons, we affirm. AFFIRMED";
        assert_eq!(detect_outcome_for_court("cafc", t), Some("respondent"));
    }

    #[test]
    fn outcome_appellate_reversed_is_petitioner() {
        let t = "We reverse the district court's grant of summary judgment. REVERSED";
        assert_eq!(detect_outcome_for_court("cafc", t), Some("petitioner"));
    }

    #[test]
    fn outcome_appellate_vacated_is_petitioner() {
        let t = "The Board's decision is vacated and the matter is remanded. VACATED";
        assert_eq!(detect_outcome_for_court("cafc", t), Some("petitioner"));
    }

    #[test]
    fn outcome_appellate_mixed_is_split() {
        // Multi-line, hyphenated form from the real corpus.
        let t = "AFFIRMED-IN-PART, VACATED-IN-PART,\n             REVERSED-IN-PART.";
        assert_eq!(detect_outcome_for_court("cafc", t), Some("split"));
    }

    #[test]
    fn outcome_appellate_in_part_words_is_split() {
        // Words-not-hyphens form from the real corpus.
        let t = "AFFIRMED IN PART AND REVERSED IN PART";
        assert_eq!(detect_outcome_for_court("cafc", t), Some("split"));
    }

    #[test]
    fn outcome_appellate_dismissed_is_none() {
        // Jurisdictional dismissal — not a merits outcome.
        let t = "we dismiss the appeal for lack of jurisdiction. DISMISSED";
        assert_eq!(detect_outcome_for_court("cafc", t), None);
    }

    #[test]
    fn outcome_appellate_only_scans_tail() {
        // Body of an opinion can reference prior appellate history ("the
        // Federal Circuit AFFIRMED in 2018, but on remand…") that must NOT
        // be confused with the current opinion's disposition.  The scanner
        // windows the last 600 chars so historical references are out of
        // scope, and an unrelated final phrase like a Rule 155 boilerplate
        // returns None.
        let body = "x".repeat(2000);
        let t = format!(
            "Long body referencing AFFIRMED on a previous appeal. {body} Decision will \
             be entered under Rule 155."
        );
        assert_eq!(detect_outcome_for_court("cafc", &t), None);
    }

    #[test]
    fn outcome_dispatch_skips_taxcourt_for_cafc_corpus() {
        // CAFC opinions sometimes recite "decision will be entered" idiom
        // when discussing a tax-court history.  detect_outcome_for_court
        // routes by court_id so the tax-court scanner doesn't fire on a
        // CAFC corpus item.
        let t = "The tax court below held: Decision will be entered for respondent. \
                 We disagree. REVERSED";
        assert_eq!(detect_outcome_for_court("cafc", t), Some("petitioner"));
    }

    // ── S15.8: court_family classification ─────────────────────────────────

    #[test]
    fn court_family_scotus() {
        assert_eq!(court_family("scotus"), CourtFamily::Scotus);
        assert_eq!(court_family("us"), CourtFamily::Scotus);
    }

    #[test]
    fn court_family_appellate() {
        assert_eq!(court_family("cafc"), CourtFamily::Appellate);
        assert_eq!(court_family("ca1"), CourtFamily::Appellate);
        assert_eq!(court_family("ca9"), CourtFamily::Appellate);
        assert_eq!(court_family("ca11"), CourtFamily::Appellate);
        assert_eq!(court_family("cadc"), CourtFamily::Appellate);
        assert_eq!(court_family("f3d"), CourtFamily::Appellate);
        assert_eq!(court_family("f4th"), CourtFamily::Appellate);
    }

    #[test]
    fn court_family_district() {
        assert_eq!(court_family("nyd"), CourtFamily::District);
        assert_eq!(court_family("cad"), CourtFamily::District);
        assert_eq!(court_family("txnd"), CourtFamily::District);
        assert_eq!(court_family("dnj"), CourtFamily::District);
    }

    #[test]
    fn court_family_tax() {
        assert_eq!(court_family("tax"), CourtFamily::Tax);
        assert_eq!(court_family("tax-court"), CourtFamily::Tax);
    }

    #[test]
    fn court_family_bankruptcy() {
        assert_eq!(court_family("bankr_sdny"), CourtFamily::Bankruptcy);
        assert_eq!(court_family("bankr.dnj"), CourtFamily::Bankruptcy);
    }

    #[test]
    fn court_family_unknown() {
        assert_eq!(court_family("mystery_court"), CourtFamily::Unknown);
        assert_eq!(court_family("statesc_ny"), CourtFamily::Unknown);
    }

    // ── S15.8: SCOTUS scanner ───────────────────────────────────────────────

    #[test]
    fn scotus_judgment_affirmed_is_respondent() {
        let t = "For the foregoing reasons, the petitioner's challenge fails. \
                 The judgment of the Court of Appeals is affirmed.";
        assert_eq!(detect_outcome_for_court("scotus", t), Some("respondent"));
    }

    #[test]
    fn scotus_reversed_and_remanded_is_petitioner() {
        // Short-form disposition — common SCOTUS per-curiam pattern.
        let preamble = "x".repeat(500);
        let t = format!("{preamble} The judgment of the Court of Appeals is reversed, \
             and the case is remanded for further proceedings consistent with this opinion.");
        assert_eq!(detect_outcome_for_court("scotus", &t), Some("petitioner"));
    }

    #[test]
    fn scotus_vacated_is_petitioner() {
        let t = "Accordingly, the decision of the Court of Appeals is vacated, \
                 and the case is remanded for further proceedings consistent with this opinion.";
        assert_eq!(detect_outcome_for_court("scotus", t), Some("petitioner"));
    }

    #[test]
    fn scotus_split_disposition() {
        let t = "The judgment of the Court of Appeals is affirmed in part \
                 and reversed in part, and the case is remanded.";
        assert_eq!(detect_outcome_for_court("scotus", t), Some("split"));
    }

    #[test]
    fn scotus_dig_is_none() {
        let t = "The writ of certiorari is dismissed as improvidently granted.";
        assert_eq!(detect_outcome_for_court("scotus", t), None);
    }

    #[test]
    fn scotus_body_dicta_does_not_match() {
        // Body mentions "the Court of Appeals affirmed in 2018" but the
        // opinion ends with no operative disposition (e.g. only a footnote
        // or further-proceedings note).  Conservative scanner: None, not a
        // guess.
        let t = "The Court of Appeals affirmed in 2018, and the petitioner sought \
                 review.  We granted certiorari to resolve a circuit split.  \
                 So ordered, with further proceedings to follow on the merits panel.";
        assert_eq!(detect_outcome_for_court("scotus", t), None);
    }

    // ── S15.8: non-CAFC appellate dispatch ──────────────────────────────────

    #[test]
    fn appellate_ca9_affirmed_is_respondent() {
        let t = "For the foregoing reasons, we affirm. AFFIRMED";
        assert_eq!(detect_outcome_for_court("ca9", t), Some("respondent"));
    }

    #[test]
    fn appellate_cadc_reversed_is_petitioner() {
        let t = "We reverse the agency's order and remand for further proceedings. REVERSED";
        assert_eq!(detect_outcome_for_court("cadc", t), Some("petitioner"));
    }

    #[test]
    fn appellate_f3d_vacated_is_petitioner() {
        let t = "The district court's order is vacated and the case remanded. VACATED";
        assert_eq!(detect_outcome_for_court("f3d", t), Some("petitioner"));
    }

    // ── S15.8: District-court scanner ───────────────────────────────────────

    #[test]
    fn district_motion_to_dismiss_granted_is_respondent() {
        let t = "For the foregoing reasons, Defendant's motion to dismiss is GRANTED. \
                 The complaint is dismissed with prejudice.";
        assert_eq!(detect_outcome_for_court("nyd", t), Some("respondent"));
    }

    #[test]
    fn district_plaintiff_msj_granted_is_petitioner() {
        let t = "For the foregoing reasons, Plaintiff's motion for summary judgment is GRANTED.";
        assert_eq!(detect_outcome_for_court("cad", t), Some("petitioner"));
    }

    #[test]
    fn district_msj_granted_in_part_is_split() {
        let t = "Defendant's motion for summary judgment is GRANTED IN PART and DENIED IN PART.";
        assert_eq!(detect_outcome_for_court("txnd", t), Some("split"));
    }

    #[test]
    fn district_no_clear_disposition_is_none() {
        let t = "The case is set for trial on the merits beginning January 10, 2026.";
        assert_eq!(detect_outcome_for_court("dnj", t), None);
    }

    #[test]
    fn district_action_dismissed_is_respondent() {
        let t = "Accordingly, the action is dismissed without prejudice for lack of subject-matter jurisdiction.";
        assert_eq!(detect_outcome_for_court("nyd", t), Some("respondent"));
    }

    #[test]
    fn bankruptcy_returns_none() {
        // Bankruptcy is deferred to Sprint 16 — even a clear-looking
        // disposition phrase routes to None.
        let t = "Defendant's motion to dismiss is GRANTED. The complaint is dismissed.";
        assert_eq!(detect_outcome_for_court("bankr_sdny", t), None);
    }
}
