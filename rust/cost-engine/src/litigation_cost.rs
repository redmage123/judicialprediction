//! First-pass litigation cost estimator (S5.10).
//!
//! Replaces the $50,000 placeholder previously hardcoded at the createCase
//! call-site in `api-gateway::graphql_predict`.  Returns a deterministic
//! [`Decimal`] cost given a jurisdiction string and procedural motion count.
//!
//! # Model (Sprint 5, first-pass)
//!
//! ```text
//! estimate_cost(j, m) = jurisdiction_base_cost(j) × (1 + MOTION_FACTOR × m)
//! ```
//!
//! Where `MOTION_FACTOR = 0.08` (8% per motion).  Empirical motion-practice
//! studies of US federal cases put per-motion cost share in the 5–10% range;
//! 8% is the middle of that band and keeps the formula simple.
//!
//! # Jurisdictions
//!
//! | Key          | Base cost  | Rationale                                          |
//! |--------------|------------|----------------------------------------------------|
//! | `us-federal` | $75,000    | Broader discovery, more motion practice            |
//! | `us-state`   | $35,000    | Narrower discovery, faster trial track             |
//! | _other_      | $50,000    | Falls back to the legacy Sprint-2/4 placeholder    |
//!
//! # Sprint 6 follow-ups
//!
//! - Per-court adjustment (not just per-jurisdiction).
//! - Expected duration factor (current model ignores trial length).
//! - Party-count factor.

use rust_decimal::Decimal;

/// Per-motion cost share, in basis points relative to base
/// (`800 bps = 0.08 = 8%`).  Kept as basis points so the constant stays
/// exact under [`Decimal`] arithmetic.
pub const MOTION_FACTOR_BPS: i64 = 800;

/// First-pass jurisdiction-base litigation cost.
///
/// Returns the legacy `$50,000` placeholder for any unrecognised
/// jurisdiction string so the cost calculation always has a sensible
/// fallback (matches the pre-S5.10 hardcoded value at the call-site).
pub fn jurisdiction_base_cost(jurisdiction: &str) -> Decimal {
    match jurisdiction {
        "us-federal" => Decimal::from(75_000_u32),
        "us-state" => Decimal::from(35_000_u32),
        _ => Decimal::from(50_000_u32),
    }
}

/// Estimate expected litigation cost in USD as an exact [`Decimal`].
///
/// `motion_count` is clamped at 50 — beyond that, the linear multiplier
/// stops being a sensible first-pass model and Sprint 6 should refine the
/// formula.  Negative motion counts are impossible (`u32`).
pub fn estimate_cost(jurisdiction: &str, motion_count: u32) -> Decimal {
    let base = jurisdiction_base_cost(jurisdiction);
    let motions = motion_count.min(50);

    // multiplier = 1 + (MOTION_FACTOR_BPS × motions) / 10_000
    //            = (10_000 + 800 × motions) / 10_000
    let numerator = Decimal::from(10_000_i64 + MOTION_FACTOR_BPS * i64::from(motions));
    let denominator = Decimal::from(10_000_i64);
    base * numerator / denominator
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().expect("test Decimal literal")
    }

    #[test]
    fn base_cost_us_federal() {
        assert_eq!(jurisdiction_base_cost("us-federal"), d("75000"));
    }

    #[test]
    fn base_cost_us_state() {
        assert_eq!(jurisdiction_base_cost("us-state"), d("35000"));
    }

    #[test]
    fn base_cost_unknown_falls_back_to_legacy_placeholder() {
        assert_eq!(jurisdiction_base_cost("ie-supreme"), d("50000"));
        assert_eq!(jurisdiction_base_cost(""), d("50000"));
    }

    /// Zero motions → base × 1.00.
    #[test]
    fn estimate_zero_motions_returns_base() {
        assert_eq!(estimate_cost("us-federal", 0), d("75000"));
        assert_eq!(estimate_cost("us-state", 0), d("35000"));
    }

    /// One motion → base × 1.08.
    #[test]
    fn estimate_one_motion_adds_eight_percent() {
        // 75,000 × 1.08 = 81,000
        assert_eq!(estimate_cost("us-federal", 1), d("81000"));
        // 35,000 × 1.08 = 37,800
        assert_eq!(estimate_cost("us-state", 1), d("37800"));
    }

    /// Five motions → base × 1.40 (5 × 8% = 40% over base).
    #[test]
    fn estimate_five_motions_adds_forty_percent() {
        // 75,000 × 1.40 = 105,000
        assert_eq!(estimate_cost("us-federal", 5), d("105000"));
        // 35,000 × 1.40 = 49,000
        assert_eq!(estimate_cost("us-state", 5), d("49000"));
    }

    /// Motion count is clamped at 50 — anything above returns the same as 50.
    #[test]
    fn estimate_clamps_motion_count_at_fifty() {
        // 75,000 × (1 + 0.08 × 50) = 75,000 × 5.0 = 375,000
        let at_cap = estimate_cost("us-federal", 50);
        let over_cap = estimate_cost("us-federal", 9_999);
        assert_eq!(at_cap, d("375000"));
        assert_eq!(at_cap, over_cap, "motion count above 50 must not exceed the cap");
    }

    /// Unknown jurisdiction still scales with motions correctly.
    #[test]
    fn estimate_unknown_jurisdiction_uses_legacy_base() {
        // 50,000 × 1.16 = 58,000
        assert_eq!(estimate_cost("ie-supreme", 2), d("58000"));
    }
}
