//! Decision-action layer — Layer 4 of the JudicialPredict pipeline (v2.14 spec §8.4).
//!
//! Pure functional-core module: no I/O, no mutable globals, no unsafe.
//! All monetary values use [`rust_decimal::Decimal`]; probabilities use `f64`.

use rust_decimal::Decimal;

// ── Public types ─────────────────────────────────────────────────────────────

/// Probabilistic prediction fed into the recommendation engine.
#[derive(Debug, Clone)]
pub struct PredictionInput {
    /// Point estimate: probability of winning at trial (0.0–1.0).
    pub p_win: f64,
    /// Lower bound of the 90% conformal confidence interval (0.0–1.0).
    pub ci_lower: f64,
    /// Upper bound of the 90% conformal confidence interval (0.0–1.0).
    pub ci_upper: f64,
    /// Expected damages if the case is won at trial (must be ≥ 0).
    pub expected_damages: Decimal,
}

/// Closed-set recommendation outcome.
///
/// Variants are mutually exclusive and evaluated in priority order inside
/// [`recommend`]:
///
/// - [`Settle`](RecommendationKind::Settle): settlement EV exceeds trial EV
///   **and** CI lower bound is below 0.40 — loss-exposure risk is material.
/// - [`Try`](RecommendationKind::Try): trial EV exceeds settlement EV **and**
///   CI lower bound exceeds 0.55 — high-confidence win case.
/// - [`Borderline`](RecommendationKind::Borderline): neither Settle nor Try
///   conditions are fully met; further negotiation analysis recommended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecommendationKind {
    /// Settlement is the dominant strategy given current predictions.
    Settle,
    /// Going to trial is the dominant strategy given current predictions.
    Try,
    /// Evidence is ambiguous; neither Settle nor Try conditions are satisfied.
    Borderline,
}

/// S6.4 — Qualitative confidence band derived from the prediction CI width.
///
/// Independent of the recommendation `kind`; lets the UI render
/// "Settle (high conf)" vs "Settle (borderline)" without recomputing
/// thresholds.  The band is purely a property of the prediction, not of the
/// recommendation rule that fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfidenceBand {
    /// CI width < 0.10 — model is tight; recommendation is robust to
    /// prediction noise.
    High,
    /// 0.10 ≤ CI width < 0.20 — middling certainty; recommendation is the
    /// best call but not insensitive to small prediction shifts.
    Medium,
    /// CI width ≥ 0.20 — the recommendation could flip if the true `p_win`
    /// landed at the edges of the CI; check `counter_recommendation` for
    /// the bound-evaluated alternative.
    Low,
}

/// S6.4 — What the recommendation would have been at the CI bounds.
///
/// `Some(...)` only when [`ConfidenceBand::Low`] (CI width ≥ 0.20); below
/// that threshold the bound-evaluated kinds are not meaningfully different
/// from the point-estimate kind and surfacing them adds noise without
/// signal.
#[derive(Debug, Clone)]
pub struct CounterRecommendation {
    /// Recommendation kind that would fire if `p_win = ci_lower`.
    pub kind_at_ci_lower: RecommendationKind,
    /// Recommendation kind that would fire if `p_win = ci_upper`.
    pub kind_at_ci_upper: RecommendationKind,
    /// Whether `kind_at_ci_lower` differs from `kind_at_ci_upper` —
    /// shorthand for "the recommendation flips inside the CI."
    pub flips_within_ci: bool,
    /// One-sentence operator-facing summary describing the flip (or its
    /// absence).  Deterministic for any given input.
    pub note: String,
}

/// Structured recommendation with expected-value comparison and reasoning bullets.
#[derive(Debug, Clone)]
pub struct Recommendation {
    /// The recommended action.
    pub kind: RecommendationKind,
    /// S6.4 — qualitative confidence band derived from CI width.
    pub confidence: ConfidenceBand,
    /// S6.4 — bound-evaluated recommendation. `Some` only when the
    /// confidence band is `Low`; `None` when the prediction is tight
    /// enough that bound-evaluated alternatives are not informative.
    pub counter_recommendation: Option<CounterRecommendation>,
    /// Three deterministic reasoning bullets. Same `PredictionInput` + same
    /// `cost` always produce the same three strings (no randomness, no I/O).
    pub rationale_bullets: [String; 3],
    /// Expected value of going to trial (`p_win × damages − cost`).
    /// May be negative when cost exceeds expected winnings.
    pub expected_value_try: Decimal,
    /// Expected value of settlement (`damages × 0.40` heuristic anchor).
    pub expected_value_settle: Decimal,
}

// ── Core function ─────────────────────────────────────────────────────────────

/// Produce a structured recommendation from a probabilistic prediction, a
/// litigation cost estimate, and the case's jurisdiction.
///
/// # Expected-value model
///
/// ```text
/// EV_try    = p_win × expected_damages − cost
/// EV_settle = settle_offer(input, jurisdiction)
///           = expected_damages × jurisdiction_settle_anchor(jurisdiction)
/// ```
///
/// `jurisdiction_settle_anchor` (S5.11) returns `0.45` for federal, `0.35`
/// for state, and `0.40` (legacy midpoint) for anything else.  Empty or
/// unknown strings keep the pre-S5.11 behaviour exactly.
///
/// # Decision rules (evaluated in order, first match wins)
///
/// 1. **`Settle`** if `EV_settle > EV_try` AND `ci_lower < 0.40`
/// 2. **`Try`** if `EV_try > EV_settle` AND `ci_lower > 0.55`
/// 3. **`Borderline`** otherwise
///
/// The `0.40` and `0.55` here are **CI thresholds** on the conformal lower
/// bound, not settle anchors — they happen to share a value with the legacy
/// anchor by coincidence, not design.
///
/// # Bullet generation
///
/// Bullets are generated deterministically: identical inputs always produce
/// identical bullet strings (verified by the bullet-stability unit tests).
///
/// # Panics
///
/// Never panics for any finite, well-formed input. `p_win` values outside
/// `[0.0, 1.0]` or non-finite floats produce `Decimal::ZERO` via the
/// `try_from` fallback and degrade gracefully to `Borderline`.
pub fn recommend(input: &PredictionInput, cost: Decimal, jurisdiction: &str) -> Recommendation {
    // Convert p_win (f64) to Decimal; fall back to 0 for NaN / ±∞.
    let p_win_dec = Decimal::try_from(input.p_win).unwrap_or(Decimal::ZERO);
    let anchor = crate::settle::jurisdiction_settle_anchor(jurisdiction);

    let ev_try = p_win_dec * input.expected_damages - cost;
    // S5.11: anchor varies by jurisdiction. settle::settle_offer is the
    // public derivation; we inline the multiplication here to avoid an
    // unnecessary clone of `input`.
    let ev_settle = input.expected_damages * anchor;

    let kind = decide_kind(input.p_win, input.ci_lower, ev_try, ev_settle);

    // S6.4 — confidence band derived purely from CI width (independent of
    // the kind that fired).
    let confidence = confidence_from_ci(input.ci_lower, input.ci_upper);

    // S6.4 — counter-recommendation only when the band is Low.  We re-run
    // the kind decision with p_win pinned to ci_lower then to ci_upper to
    // see whether the recommendation would flip at the edges of the CI.
    // EVs are recomputed at each bound because ev_try depends on p_win.
    let counter_recommendation = if confidence == ConfidenceBand::Low {
        let ev_try_lo = Decimal::try_from(input.ci_lower).unwrap_or(Decimal::ZERO)
            * input.expected_damages
            - cost;
        let ev_try_hi = Decimal::try_from(input.ci_upper).unwrap_or(Decimal::ZERO)
            * input.expected_damages
            - cost;
        let kind_lo = decide_kind(input.ci_lower, input.ci_lower, ev_try_lo, ev_settle);
        let kind_hi = decide_kind(input.ci_upper, input.ci_upper, ev_try_hi, ev_settle);
        let flips = kind_lo != kind_hi;
        let note = if flips {
            format!(
                "Recommendation flips inside the 90% CI: at p_win={:.2} (lower) → {}; \
                 at p_win={:.2} (upper) → {}.  Treat as advisory only.",
                input.ci_lower,
                kind_label(&kind_lo),
                input.ci_upper,
                kind_label(&kind_hi),
            )
        } else {
            format!(
                "Both CI bounds agree on {}; band is Low purely from CI width, not \
                 from disagreement between bounds.",
                kind_label(&kind_lo),
            )
        };
        Some(CounterRecommendation {
            kind_at_ci_lower: kind_lo,
            kind_at_ci_upper: kind_hi,
            flips_within_ci: flips,
            note,
        })
    } else {
        None
    };

    let bullet1 = format!(
        "P(win) {:.2} with 90% CI [{:.2}, {:.2}] — {}",
        input.p_win,
        input.ci_lower,
        input.ci_upper,
        confidence_label(&confidence),
    );
    // S6.4 — bullet 2 now exposes the Nash-Rubinstein anchor derivation so
    // the EV comparison is auditable end-to-end.  Format:
    //   "Expected value at trial $X (p_win × damages − cost) vs.
    //    expected settlement value $Y (anchor A × damages, <juris> prior)"
    let jurisdiction_label = if jurisdiction.is_empty() {
        "legacy"
    } else {
        jurisdiction
    };
    let bullet2 = format!(
        "Expected value at trial ${} (p_win × damages − cost) vs. expected settlement \
         value ${} (Nash anchor {} × damages, {} prior)",
        ev_try.round_dp(2),
        ev_settle.round_dp(2),
        anchor.round_dp(2),
        jurisdiction_label,
    );
    let bullet3 = match &kind {
        RecommendationKind::Settle => format!(
            "Settlement preferred: CI lower bound ({:.2}) is below the loss-exposure \
             threshold of 0.40 and settlement EV (${}) exceeds trial EV (${})",
            input.ci_lower,
            ev_settle.round_dp(2),
            ev_try.round_dp(2),
        ),
        RecommendationKind::Try => format!(
            "Trial expected value (${}) exceeds settlement (${}) and lower CI bound \
             ({:.2}) is above the trial-justification threshold of 0.55",
            ev_try.round_dp(2),
            ev_settle.round_dp(2),
            input.ci_lower,
        ),
        RecommendationKind::Borderline => format!(
            "Outcome is borderline: CI lower bound ({:.2}) falls between thresholds \
             (0.40–0.55) or EV comparison does not clearly favor trial or settlement",
            input.ci_lower,
        ),
    };

    Recommendation {
        kind,
        confidence,
        counter_recommendation,
        rationale_bullets: [bullet1, bullet2, bullet3],
        expected_value_try: ev_try,
        expected_value_settle: ev_settle,
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Apply the S5.11 decision rules.  Pure function; extracted so the
/// recommender can re-invoke at the CI bounds for `counter_recommendation`
/// without duplicating the threshold logic.
fn decide_kind(
    _p_win: f64,
    ci_lower: f64,
    ev_try: Decimal,
    ev_settle: Decimal,
) -> RecommendationKind {
    if ev_settle > ev_try && ci_lower < 0.40 {
        RecommendationKind::Settle
    } else if ev_try > ev_settle && ci_lower > 0.55 {
        RecommendationKind::Try
    } else {
        RecommendationKind::Borderline
    }
}

/// Width-keyed confidence band.  Saturates ci_upper to [0, 1] and
/// ci_lower to [0, 1] before differencing so out-of-range inputs land in
/// the same bucket they would have without the OOR slop.
fn confidence_from_ci(ci_lower: f64, ci_upper: f64) -> ConfidenceBand {
    let lo = ci_lower.clamp(0.0, 1.0);
    let hi = ci_upper.clamp(0.0, 1.0);
    let width = (hi - lo).abs();
    if width < 0.10 {
        ConfidenceBand::High
    } else if width < 0.20 {
        ConfidenceBand::Medium
    } else {
        ConfidenceBand::Low
    }
}

fn confidence_label(b: &ConfidenceBand) -> &'static str {
    match b {
        ConfidenceBand::High => "high confidence",
        ConfidenceBand::Medium => "medium confidence",
        ConfidenceBand::Low => "low confidence (recommendation may flip — see counter)",
    }
}

fn kind_label(k: &RecommendationKind) -> &'static str {
    match k {
        RecommendationKind::Settle => "Settle",
        RecommendationKind::Try => "Try",
        RecommendationKind::Borderline => "Borderline",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Convenience: parse a Decimal from a string literal.
    fn d(s: &str) -> Decimal {
        s.parse().expect("test Decimal literal")
    }

    // ── Branch coverage: one test per RecommendationKind ────────────────────

    /// Settle branch: EV_settle > EV_try AND ci_lower < 0.40.
    ///
    /// p_win=0.30, damages=$100,000, cost=$50,000
    ///   EV_try    = 0.30 × 100,000 − 50,000 = −20,000
    ///   EV_settle = 100,000 × 0.40          =  40,000  (> EV_try)
    ///   ci_lower  = 0.30  (<0.40) ✓
    #[test]
    fn settle_when_ev_settle_dominates_and_low_ci() {
        let input = PredictionInput {
            p_win: 0.30,
            ci_lower: 0.30,
            ci_upper: 0.50,
            expected_damages: d("100000.00"),
        };
        // Empty jurisdiction → legacy 0.40 anchor; preserves the pre-S5.11
        // expected values so this assertion still pins the unchanged path.
        let rec = recommend(&input, d("50000.00"), "");
        assert_eq!(rec.kind, RecommendationKind::Settle);
        assert_eq!(rec.expected_value_try, d("-20000.00"));
        assert_eq!(rec.expected_value_settle, d("40000.0000"));
    }

    /// S5.11: jurisdiction shifts the settle anchor, which can flip the
    /// recommendation kind.  Same prediction + same cost + us-federal anchor
    /// (0.45) raises EV_settle from $40k to $45k.  With p_win=0.30 the trial
    /// EV stays at −$20k so Settle still dominates EV-wise, but the bullet
    /// numbers shift.
    #[test]
    fn settle_anchor_varies_by_jurisdiction() {
        let input = PredictionInput {
            p_win: 0.30,
            ci_lower: 0.30,
            ci_upper: 0.50,
            expected_damages: d("100000.00"),
        };
        let federal = recommend(&input, d("50000.00"), "us-federal");
        let state = recommend(&input, d("50000.00"), "us-state");
        assert_eq!(federal.expected_value_settle, d("45000.0000"));
        assert_eq!(state.expected_value_settle, d("35000.0000"));
        // EV_try is unaffected by jurisdiction.
        assert_eq!(federal.expected_value_try, d("-20000.00"));
        assert_eq!(state.expected_value_try, d("-20000.00"));
    }

    /// Try branch: EV_try > EV_settle AND ci_lower > 0.55.
    ///
    /// p_win=0.80, damages=$100,000, cost=$10,000
    ///   EV_try    = 0.80 × 100,000 − 10,000 = 70,000  (> EV_settle)
    ///   EV_settle = 100,000 × 0.40           = 40,000
    ///   ci_lower  = 0.65  (>0.55) ✓
    #[test]
    fn try_when_ev_try_dominates_and_high_ci() {
        let input = PredictionInput {
            p_win: 0.80,
            ci_lower: 0.65,
            ci_upper: 0.92,
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("10000.00"), "");
        assert_eq!(rec.kind, RecommendationKind::Try);
        assert_eq!(rec.expected_value_try, d("70000.00"));
        assert_eq!(rec.expected_value_settle, d("40000.0000"));
    }

    // ── Boundary-equality tests (S5.12) ────────────────────────────────────
    //
    // The decision rules use **strict** inequalities on both ci_lower and the
    // EV comparison.  These tests pin every boundary so that a mutation from
    // `<` to `<=` or `>` to `>=` (which cargo-mutants will routinely try)
    // fails the suite immediately.  Without these, a flipped boundary
    // produces a wrong recommendation for cases that land exactly on the
    // threshold — a small but legally meaningful drift.

    /// `ci_lower == 0.40` exactly → Borderline, NOT Settle.
    /// Catches the `<` → `<=` mutation in the Settle branch.
    #[test]
    fn boundary_ci_lower_eq_0_40_is_borderline_not_settle() {
        let input = PredictionInput {
            p_win: 0.30,
            ci_lower: 0.40, // exactly on the loss-exposure threshold
            ci_upper: 0.50,
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("50000.00"), "");
        assert_eq!(
            rec.kind,
            RecommendationKind::Borderline,
            "ci_lower == 0.40 must be Borderline (strict `<`), got {:?}",
            rec.kind,
        );
    }

    /// `ci_lower == 0.55` exactly → Borderline, NOT Try.
    /// Catches the `>` → `>=` mutation in the Try branch.
    #[test]
    fn boundary_ci_lower_eq_0_55_is_borderline_not_try() {
        let input = PredictionInput {
            p_win: 0.80,
            ci_lower: 0.55, // exactly on the trial-justification threshold
            ci_upper: 0.92,
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("10000.00"), "");
        assert_eq!(
            rec.kind,
            RecommendationKind::Borderline,
            "ci_lower == 0.55 must be Borderline (strict `>`), got {:?}",
            rec.kind,
        );
    }

    /// `ev_settle == ev_try` exactly with ci_lower < 0.40 → Borderline.
    /// The Settle rule requires `ev_settle > ev_try` (strict); equal EVs
    /// must NOT trigger Settle even with low CI.  Catches the EV
    /// comparison `>` → `>=` mutation.
    #[test]
    fn boundary_ev_equal_is_borderline_not_settle() {
        // EV_settle = damages × 0.40 = 40,000
        // Pick p_win + cost so EV_try = 40,000 too:
        //   ev_try = 0.90 × 100,000 − 50,000 = 40,000
        let input = PredictionInput {
            p_win: 0.90,
            ci_lower: 0.30, // < 0.40 (would satisfy Settle's CI rule)
            ci_upper: 0.95,
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("50000.00"), "");
        assert_eq!(rec.expected_value_try, d("40000.00"));
        assert_eq!(rec.expected_value_settle, d("40000.0000"));
        assert_eq!(
            rec.kind,
            RecommendationKind::Borderline,
            "ev_settle == ev_try must be Borderline (strict `>`), got {:?}",
            rec.kind,
        );
    }

    /// `ev_try == ev_settle` exactly with ci_lower > 0.55 → Borderline.
    /// The Try rule requires `ev_try > ev_settle` (strict); equal EVs
    /// must NOT trigger Try even with high CI.
    #[test]
    fn boundary_ev_equal_is_borderline_not_try() {
        // Same EV setup as the previous test, but with ci_lower > 0.55.
        let input = PredictionInput {
            p_win: 0.90,
            ci_lower: 0.60, // > 0.55 (would satisfy Try's CI rule)
            ci_upper: 0.95,
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("50000.00"), "");
        assert_eq!(rec.expected_value_try, d("40000.00"));
        assert_eq!(rec.expected_value_settle, d("40000.0000"));
        assert_eq!(
            rec.kind,
            RecommendationKind::Borderline,
            "ev_try == ev_settle must be Borderline (strict `>`), got {:?}",
            rec.kind,
        );
    }

    /// Just-below threshold: `ci_lower = 0.40 - ε` still triggers Settle.
    /// Sanity-check the other side of the boundary so the equality tests
    /// above don't paper over a broader regression.
    #[test]
    fn just_below_0_40_threshold_still_settles() {
        let input = PredictionInput {
            p_win: 0.30,
            ci_lower: 0.40 - 1e-9, // just under the threshold
            ci_upper: 0.50,
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("50000.00"), "");
        assert_eq!(rec.kind, RecommendationKind::Settle);
    }

    /// Just-above threshold: `ci_lower = 0.55 + ε` still triggers Try.
    #[test]
    fn just_above_0_55_threshold_still_tries() {
        let input = PredictionInput {
            p_win: 0.80,
            ci_lower: 0.55 + 1e-9, // just over the threshold
            ci_upper: 0.92,
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("10000.00"), "");
        assert_eq!(rec.kind, RecommendationKind::Try);
    }

    // ── End boundary-equality tests ────────────────────────────────────────

    /// Borderline branch: EV comparison favours settle but ci_lower ≥ 0.40,
    /// so the Settle rule is not triggered; Try also fails.
    ///
    /// p_win=0.50, damages=$100,000, cost=$45,000
    ///   EV_try    = 0.50 × 100,000 − 45,000 = 5,000
    ///   EV_settle = 100,000 × 0.40           = 40,000  (> EV_try)
    ///   ci_lower  = 0.45  (not < 0.40 → Settle fails; not > 0.55 → Try fails)
    ///   → Borderline
    #[test]
    fn borderline_when_ci_is_in_ambiguous_zone() {
        let input = PredictionInput {
            p_win: 0.50,
            ci_lower: 0.45,
            ci_upper: 0.60,
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("45000.00"), "");
        assert_eq!(rec.kind, RecommendationKind::Borderline);
    }

    // ── Bullet stability: same inputs → identical bullets ───────────────────

    /// Two calls with the Settle input produce byte-identical bullet strings.
    #[test]
    fn bullets_stable_for_settle_variant() {
        let input = PredictionInput {
            p_win: 0.25,
            ci_lower: 0.18,
            ci_upper: 0.44,
            expected_damages: d("250000.00"),
        };
        let cost = d("80000.00");
        let r1 = recommend(&input, cost, "");
        let r2 = recommend(&input, cost, "");
        assert_eq!(
            r1.rationale_bullets, r2.rationale_bullets,
            "bullets must be deterministic across identical calls",
        );
    }

    /// Two calls with the Try input produce byte-identical bullet strings.
    #[test]
    fn bullets_stable_for_try_variant() {
        let input = PredictionInput {
            p_win: 0.75,
            ci_lower: 0.62,
            ci_upper: 0.88,
            expected_damages: d("500000.00"),
        };
        let cost = d("50000.00");
        let r1 = recommend(&input, cost, "");
        let r2 = recommend(&input, cost, "");
        assert_eq!(
            r1.rationale_bullets, r2.rationale_bullets,
            "bullets must be deterministic across identical calls",
        );
    }

    // ── Bullet content: verify expected substrings ───────────────────────────

    /// Bullet 0 must contain the formatted p_win value "P(win) 0.42".
    #[test]
    fn bullet_one_contains_p_win_formatted() {
        let input = PredictionInput {
            p_win: 0.42,
            ci_lower: 0.31,
            ci_upper: 0.53,
            expected_damages: d("80000.00"),
        };
        let rec = recommend(&input, d("20000.00"), "");
        assert!(
            rec.rationale_bullets[0].contains("P(win) 0.42"),
            "bullet[0] missing 'P(win) 0.42': got {:?}",
            rec.rationale_bullets[0],
        );
        assert!(
            rec.rationale_bullets[0].contains("0.31"),
            "bullet[0] missing ci_lower '0.31': got {:?}",
            rec.rationale_bullets[0],
        );
        assert!(
            rec.rationale_bullets[0].contains("0.53"),
            "bullet[0] missing ci_upper '0.53': got {:?}",
            rec.rationale_bullets[0],
        );
        assert!(
            rec.rationale_bullets[1].contains("Expected value at trial"),
            "bullet[1] missing EV preamble: got {:?}",
            rec.rationale_bullets[1],
        );
    }

    // ── Property test: no panics, always 3 non-empty bullets ─────────────────

    // ── S6.4: ConfidenceBand + counter_recommendation ───────────────────────

    /// Tight CI (< 0.10 width) → High band, no counter-recommendation.
    #[test]
    fn confidence_high_when_ci_tight() {
        let input = PredictionInput {
            p_win: 0.80,
            ci_lower: 0.77,
            ci_upper: 0.83, // width 0.06
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("10000.00"), "");
        assert_eq!(rec.confidence, ConfidenceBand::High);
        assert!(rec.counter_recommendation.is_none());
    }

    /// Medium-width CI (0.10–0.20) → Medium band, no counter.
    #[test]
    fn confidence_medium_when_ci_moderate() {
        let input = PredictionInput {
            p_win: 0.50,
            ci_lower: 0.42,
            ci_upper: 0.56, // width 0.14
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("50000.00"), "");
        assert_eq!(rec.confidence, ConfidenceBand::Medium);
        assert!(rec.counter_recommendation.is_none());
    }

    /// Wide CI (≥ 0.20 width) → Low band; counter MUST be populated.
    #[test]
    fn confidence_low_carries_counter_recommendation() {
        let input = PredictionInput {
            p_win: 0.50,
            ci_lower: 0.30,
            ci_upper: 0.70, // width 0.40
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("45000.00"), "");
        assert_eq!(rec.confidence, ConfidenceBand::Low);
        assert!(rec.counter_recommendation.is_some(), "Low band must carry a counter");
    }

    /// When the CI is wide enough that ci_lower triggers Settle but ci_upper
    /// triggers Try, `flips_within_ci` must be true.
    #[test]
    fn counter_flips_when_bounds_disagree() {
        let input = PredictionInput {
            p_win: 0.50,
            ci_lower: 0.30,   // < 0.40 → Settle territory at lower bound
            ci_upper: 0.85,   // > 0.55 → Try territory at upper bound
            expected_damages: d("100000.00"),
        };
        // Cost picked so EV ordering matches the CI-bound regimes at each end.
        let rec = recommend(&input, d("10000.00"), "");
        let counter = rec.counter_recommendation.expect("Low band must have counter");
        assert!(counter.flips_within_ci);
        assert_eq!(counter.kind_at_ci_lower, RecommendationKind::Settle);
        assert_eq!(counter.kind_at_ci_upper, RecommendationKind::Try);
        assert!(counter.note.contains("flips"));
    }

    /// CI width ≥ 0.20 but both bounds land in Borderline → counter present
    /// but `flips_within_ci` is false; note says "agree on Borderline".
    #[test]
    fn counter_does_not_flip_when_both_bounds_agree() {
        let input = PredictionInput {
            p_win: 0.50,
            ci_lower: 0.42,
            ci_upper: 0.54, // 0.12 width → Medium, no counter
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("45000.00"), "");
        // Sanity: this is Medium, not Low — confirms we don't synthesise a counter.
        assert_eq!(rec.confidence, ConfidenceBand::Medium);
        assert!(rec.counter_recommendation.is_none());
    }

    /// Bullet 0 now carries a confidence label suffix.
    #[test]
    fn bullet_0_carries_confidence_label() {
        let high = PredictionInput {
            p_win: 0.80,
            ci_lower: 0.78,
            ci_upper: 0.83,
            expected_damages: d("100000.00"),
        };
        assert!(recommend(&high, d("10000.00"), "").rationale_bullets[0]
            .contains("high confidence"));

        let low = PredictionInput {
            p_win: 0.50,
            ci_lower: 0.30,
            ci_upper: 0.70,
            expected_damages: d("100000.00"),
        };
        assert!(recommend(&low, d("45000.00"), "").rationale_bullets[0]
            .contains("low confidence"));
    }

    /// Bullet 1 now exposes the Nash-Rubinstein anchor derivation.
    #[test]
    fn bullet_1_exposes_nash_anchor() {
        let input = PredictionInput {
            p_win: 0.30,
            ci_lower: 0.20,
            ci_upper: 0.40,
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("20000.00"), "us-federal");
        let b = &rec.rationale_bullets[1];
        assert!(b.contains("Nash anchor"), "bullet 1 missing Nash anchor: {b}");
        assert!(b.contains("0.45"), "bullet 1 missing federal anchor 0.45: {b}");
        assert!(b.contains("us-federal"), "bullet 1 missing jurisdiction label: {b}");
        assert!(
            b.contains("p_win × damages − cost"),
            "bullet 1 missing EV-try derivation: {b}",
        );
    }

    /// Empty-jurisdiction → "legacy" label in bullet 1 (preserves the
    /// "anchor is 0.40 legacy fallback" contract from S5.11).
    #[test]
    fn bullet_1_uses_legacy_label_for_empty_jurisdiction() {
        let input = PredictionInput {
            p_win: 0.30,
            ci_lower: 0.20,
            ci_upper: 0.40,
            expected_damages: d("100000.00"),
        };
        let rec = recommend(&input, d("20000.00"), "");
        assert!(rec.rationale_bullets[1].contains("legacy prior"));
        assert!(rec.rationale_bullets[1].contains("0.40"));
    }

    // ── End S6.4 tests ───────────────────────────────────────────────────────

    proptest! {
        /// For any well-formed `PredictionInput` and `cost ∈ [0, expected_damages]`,
        /// `recommend` must:
        /// - return without panicking,
        /// - produce exactly one of the three variants (enforced by the closed enum),
        /// - have all three bullet strings non-empty.
        #[test]
        fn prop_recommend_never_panics_any_valid_input(
            p_win      in 0.0_f64..=1.0_f64,
            ci_lower   in 0.0_f64..=1.0_f64,
            ci_upper   in 0.0_f64..=1.0_f64,
            // Generate damages and cost as integer cents to stay in exact Decimal.
            damages_cents in 0_i64..=10_000_000_00_i64,
            // cost_cents ≤ damages_cents guarantees cost ∈ [0, expected_damages].
            cost_frac_pct in 0_u32..=100_u32,
        ) {
            let expected_damages = Decimal::new(damages_cents, 2);
            // cost = damages × (cost_frac_pct / 100), exact integer arithmetic.
            let cost = expected_damages
                * Decimal::new(i64::from(cost_frac_pct), 2);

            let input = PredictionInput { p_win, ci_lower, ci_upper, expected_damages };
            let rec = recommend(&input, cost, "");

            // All three bullets must be non-empty strings.
            prop_assert!(!rec.rationale_bullets[0].is_empty(), "bullet[0] is empty");
            prop_assert!(!rec.rationale_bullets[1].is_empty(), "bullet[1] is empty");
            prop_assert!(!rec.rationale_bullets[2].is_empty(), "bullet[2] is empty");
        }
    }
}
