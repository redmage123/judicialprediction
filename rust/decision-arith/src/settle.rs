//! Settlement-anchor model (S5.11).
//!
//! Replaces the fixed `0.40` anchor previously hardcoded in [`recommend`] with
//! a jurisdiction-keyed anchor.  The function name `settle_offer` keeps the
//! spec language: this is the "Rubinstein-style" anchor described in v2.14
//! §5.4 — a first-pass model that varies by jurisdiction.  Sprint 6+ will
//! refine with BATNA/WATNA/ZOPA from operator intake.
//!
//! # Anchors
//!
//! | Jurisdiction | Anchor | Rationale                                              |
//! |--------------|--------|--------------------------------------------------------|
//! | `us-federal` | 0.45   | Federal damages settle higher (broader discovery cost) |
//! | `us-state`   | 0.35   | Faster trial track keeps anchor lower                  |
//! | _other_      | 0.40   | Legacy midpoint of the 30–50% empirical band           |
//!
//! [`recommend`]: crate::recommend

use rust_decimal::Decimal;

use crate::recommend::PredictionInput;

/// Settle-anchor fraction of expected damages, keyed by jurisdiction string.
///
/// The unknown-jurisdiction fallback `0.40` matches the pre-S5.11 hardcoded
/// value used everywhere — so passing an empty or unrecognised string keeps
/// the legacy behaviour exactly.
pub fn jurisdiction_settle_anchor(jurisdiction: &str) -> Decimal {
    match jurisdiction {
        "us-federal" => Decimal::new(45, 2), // 0.45
        "us-state" => Decimal::new(35, 2),   // 0.35
        _ => Decimal::new(40, 2),            // 0.40 (legacy)
    }
}

/// Expected settlement value (USD) given a prediction and jurisdiction.
///
/// `settle_offer(input, "us-federal") == input.expected_damages × 0.45`
///
/// Returned as an exact [`Decimal`].  This is the same number the recommender
/// uses as `EV_settle`; exposing it as a standalone fn lets the API gateway
/// surface it directly when the caller only wants the anchor (no full
/// recommendation).
pub fn settle_offer(input: &PredictionInput, jurisdiction: &str) -> Decimal {
    input.expected_damages * jurisdiction_settle_anchor(jurisdiction)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().expect("test Decimal literal")
    }

    #[test]
    fn anchor_us_federal_is_zero_point_four_five() {
        assert_eq!(jurisdiction_settle_anchor("us-federal"), d("0.45"));
    }

    #[test]
    fn anchor_us_state_is_zero_point_three_five() {
        assert_eq!(jurisdiction_settle_anchor("us-state"), d("0.35"));
    }

    #[test]
    fn anchor_unknown_falls_back_to_legacy_zero_point_four() {
        assert_eq!(jurisdiction_settle_anchor("ie-supreme"), d("0.40"));
        assert_eq!(jurisdiction_settle_anchor(""), d("0.40"));
    }

    #[test]
    fn settle_offer_scales_damages_by_anchor() {
        let input = PredictionInput {
            p_win: 0.6,
            ci_lower: 0.45,
            ci_upper: 0.75,
            expected_damages: d("100000.00"),
        };
        assert_eq!(settle_offer(&input, "us-federal"), d("45000.0000"));
        assert_eq!(settle_offer(&input, "us-state"), d("35000.0000"));
        assert_eq!(settle_offer(&input, "ie-supreme"), d("40000.0000"));
    }
}
