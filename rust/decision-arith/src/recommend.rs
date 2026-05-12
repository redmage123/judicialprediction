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

/// Structured recommendation with expected-value comparison and reasoning bullets.
#[derive(Debug, Clone)]
pub struct Recommendation {
    /// The recommended action.
    pub kind: RecommendationKind,
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

    let ev_try = p_win_dec * input.expected_damages - cost;
    // S5.11: anchor varies by jurisdiction. settle::settle_offer is the
    // public derivation; we inline the multiplication here to avoid an
    // unnecessary clone of `input`.
    let ev_settle = input.expected_damages * crate::settle::jurisdiction_settle_anchor(jurisdiction);

    let kind = if ev_settle > ev_try && input.ci_lower < 0.40 {
        RecommendationKind::Settle
    } else if ev_try > ev_settle && input.ci_lower > 0.55 {
        RecommendationKind::Try
    } else {
        RecommendationKind::Borderline
    };

    let bullet1 = format!(
        "P(win) {:.2} with 90% CI [{:.2}, {:.2}]",
        input.p_win, input.ci_lower, input.ci_upper,
    );
    let bullet2 = format!(
        "Expected value at trial ${} vs. expected settlement value ${}",
        ev_try.round_dp(2),
        ev_settle.round_dp(2),
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
        rationale_bullets: [bullet1, bullet2, bullet3],
        expected_value_try: ev_try,
        expected_value_settle: ev_settle,
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
