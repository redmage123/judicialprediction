//! Litigation cost estimator — S5.10 first-pass, S6.7 v2.
//!
//! Returns a deterministic [`Decimal`] cost.  S5.10 shipped a first-pass
//! model (jurisdiction-base × motion-count); the Sprint-5 risk plan flagged
//! that as a simplification to fix.  S6.7 layers two more factors on top:
//! expected case duration and party count.
//!
//! # Model (S6.7 v2)
//!
//! ```text
//! estimate_cost_v2 = jurisdiction_base(j)
//!     × (1 + MOTION_FACTOR    × motions)
//!     × (1 + DURATION_FACTOR  × months_over_baseline)
//!     × (1 + PARTY_FACTOR     × parties_over_baseline)
//! ```
//!
//! Each factor only ever *raises* cost: a case shorter than the baseline
//! duration, or with the baseline party count, contributes a ×1.0 multiplier
//! rather than a discount.  That keeps the model conservative and sidesteps
//! a multiplier ever going to zero or negative.
//!
//! | Factor          | Constant            | Baseline | Rationale                                              |
//! |-----------------|---------------------|----------|--------------------------------------------------------|
//! | Motion practice | 8% / motion         | 0        | Empirical motion-cost share is 5–10%; 8% is mid-band.  |
//! | Expected duration | 5% / month over 12 | 12 mo   | Attorney time scales ~linearly with calendar length.  |
//! | Party count     | 15% / party over 2  | 2        | Multi-party adds depositions, discovery, cross-motions.|
//!
//! `estimate_cost` (v1) is preserved as a thin wrapper over v2 at the
//! duration + party baselines, so its output — and the S5.10 tests — are
//! byte-for-byte unchanged.
//!
//! # Jurisdictions
//!
//! | Key          | Base cost  | Rationale                                       |
//! |--------------|------------|-------------------------------------------------|
//! | `us-federal` | $75,000    | Broader discovery, more motion practice         |
//! | `us-state`   | $35,000    | Narrower discovery, faster trial track          |
//! | _other_      | $50,000    | Falls back to the legacy Sprint-2/4 placeholder |

use rust_decimal::Decimal;

/// Per-motion cost share, in basis points relative to base
/// (`800 bps = 0.08 = 8%`).  Kept as basis points so the constant stays
/// exact under [`Decimal`] arithmetic.
pub const MOTION_FACTOR_BPS: i64 = 800;

/// Per-month cost share for every month a case runs beyond
/// [`BASELINE_DURATION_MONTHS`] (`500 bps = 5%`).
pub const DURATION_FACTOR_BPS: i64 = 500;

/// Per-party cost share for every party beyond [`BASELINE_PARTY_COUNT`]
/// (`1500 bps = 15%`).
pub const PARTY_FACTOR_BPS: i64 = 1_500;

/// Expected case length, in months, that contributes no duration premium.
/// A typical US civil case from filing to disposition runs about a year.
pub const BASELINE_DURATION_MONTHS: u32 = 12;

/// Party count that contributes no multi-party premium — one plaintiff,
/// one defendant.
pub const BASELINE_PARTY_COUNT: u32 = 2;

/// Upper clamp on motion count before the linear model stops being sensible.
const MOTION_COUNT_CAP: u32 = 50;

/// Upper clamp on expected duration (5 years).  Beyond this the linear
/// premium overstates cost and a richer model is needed.
const DURATION_MONTHS_CAP: u32 = 60;

/// Upper clamp on party count.  Genuinely massive multi-party litigation
/// (mass torts, class actions) needs its own model, not this linear one.
const PARTY_COUNT_CAP: u32 = 20;

const BPS_DENOMINATOR: i64 = 10_000;

/// Inputs to the S6.7 v2 cost model.
///
/// `jurisdiction` selects the base cost; the three numeric fields each
/// contribute a multiplicative premium.  Callers that only have Tier-A/B
/// data can derive `expected_duration_months` via
/// [`derive_duration_months`] and pass [`BASELINE_PARTY_COUNT`] for
/// `party_count` until richer intake data is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostInputs<'a> {
    /// Jurisdiction key — see the table in the module docs.
    pub jurisdiction: &'a str,
    /// Number of procedural motions filed.  Clamped at 50.
    pub motion_count: u32,
    /// Expected case length in months.  Clamped at 60; values at or below
    /// [`BASELINE_DURATION_MONTHS`] add no premium.
    pub expected_duration_months: u32,
    /// Number of parties to the case.  Clamped at 20; values at or below
    /// [`BASELINE_PARTY_COUNT`] add no premium.
    pub party_count: u32,
}

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

/// One multiplicative factor as an exact `(10_000 + factor_bps × delta)`
/// numerator over the shared `10_000` denominator.  `delta` is the clamped
/// amount by which the input exceeds its baseline; a zero `delta` yields a
/// numerator of exactly `10_000`, i.e. a ×1.0 multiplier.
fn factor_numerator(factor_bps: i64, delta: u32) -> Decimal {
    Decimal::from(BPS_DENOMINATOR + factor_bps * i64::from(delta))
}

/// Derive an expected case duration (months) from the procedural motion
/// count, for callers that have no explicit duration signal.
///
/// Heuristic: start at the [`BASELINE_DURATION_MONTHS`] baseline and add one
/// month of calendar time per motion (briefing → response → hearing →
/// ruling realistically consumes about that).  Clamped at the v2 duration
/// cap.  Documented here so the gateway has a single place to point at.
pub fn derive_duration_months(motion_count: u32) -> u32 {
    (BASELINE_DURATION_MONTHS + motion_count).min(DURATION_MONTHS_CAP)
}

/// Estimate expected litigation cost in USD as an exact [`Decimal`] — S6.7 v2.
///
/// All three numeric inputs are clamped (see [`CostInputs`]); each
/// contributes a multiplicative premium only when it exceeds its baseline.
pub fn estimate_cost_v2(inputs: &CostInputs<'_>) -> Decimal {
    let base = jurisdiction_base_cost(inputs.jurisdiction);

    let motions = inputs.motion_count.min(MOTION_COUNT_CAP);
    // saturating_sub: inputs at or below baseline contribute a zero delta.
    let extra_months = inputs
        .expected_duration_months
        .min(DURATION_MONTHS_CAP)
        .saturating_sub(BASELINE_DURATION_MONTHS);
    let extra_parties = inputs
        .party_count
        .min(PARTY_COUNT_CAP)
        .saturating_sub(BASELINE_PARTY_COUNT);

    let denominator = Decimal::from(BPS_DENOMINATOR);
    // Apply each factor stepwise: base × numerator / 10_000.  Every
    // intermediate is an exact Decimal (whole-number numerators), so the
    // result carries no floating-point error.
    let mut cost = base * factor_numerator(MOTION_FACTOR_BPS, motions) / denominator;
    cost = cost * factor_numerator(DURATION_FACTOR_BPS, extra_months) / denominator;
    cost = cost * factor_numerator(PARTY_FACTOR_BPS, extra_parties) / denominator;
    cost
}

/// Estimate expected litigation cost — S5.10 v1 signature, preserved.
///
/// Thin wrapper over [`estimate_cost_v2`] at the duration and party
/// baselines, so both factors collapse to ×1.0 and the output is identical
/// to the S5.10 formula.  Callers with no duration/party signal can keep
/// using this; callers that have richer inputs should call
/// [`estimate_cost_v2`] directly.
pub fn estimate_cost(jurisdiction: &str, motion_count: u32) -> Decimal {
    estimate_cost_v2(&CostInputs {
        jurisdiction,
        motion_count,
        expected_duration_months: BASELINE_DURATION_MONTHS,
        party_count: BASELINE_PARTY_COUNT,
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().expect("test Decimal literal")
    }

    /// CostInputs at both v2 baselines — duration + party factors are ×1.0.
    fn baseline_inputs(jurisdiction: &str, motion_count: u32) -> CostInputs<'_> {
        CostInputs {
            jurisdiction,
            motion_count,
            expected_duration_months: BASELINE_DURATION_MONTHS,
            party_count: BASELINE_PARTY_COUNT,
        }
    }

    // ── S5.10 v1 regression: estimate_cost output must be unchanged ──────────

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
        assert_eq!(estimate_cost("us-federal", 1), d("81000"));
        assert_eq!(estimate_cost("us-state", 1), d("37800"));
    }

    /// Five motions → base × 1.40 (5 × 8% = 40% over base).
    #[test]
    fn estimate_five_motions_adds_forty_percent() {
        assert_eq!(estimate_cost("us-federal", 5), d("105000"));
        assert_eq!(estimate_cost("us-state", 5), d("49000"));
    }

    /// Motion count is clamped at 50 — anything above returns the same as 50.
    #[test]
    fn estimate_clamps_motion_count_at_fifty() {
        let at_cap = estimate_cost("us-federal", 50);
        let over_cap = estimate_cost("us-federal", 9_999);
        assert_eq!(at_cap, d("375000"));
        assert_eq!(at_cap, over_cap, "motion count above 50 must not exceed the cap");
    }

    /// Unknown jurisdiction still scales with motions correctly.
    #[test]
    fn estimate_unknown_jurisdiction_uses_legacy_base() {
        assert_eq!(estimate_cost("ie-supreme", 2), d("58000"));
    }

    // ── S6.7 v2: duration factor ────────────────────────────────────────────

    /// Duration at or below the 12-month baseline adds no premium.
    #[test]
    fn v2_duration_at_baseline_is_neutral() {
        let at = estimate_cost_v2(&CostInputs {
            expected_duration_months: BASELINE_DURATION_MONTHS,
            ..baseline_inputs("us-federal", 0)
        });
        let below = estimate_cost_v2(&CostInputs {
            expected_duration_months: 3,
            ..baseline_inputs("us-federal", 0)
        });
        assert_eq!(at, d("75000"));
        assert_eq!(below, d("75000"), "shorter-than-baseline must not discount");
    }

    /// 24 months → 12 months over baseline × 5% = +60% → base × 1.60.
    #[test]
    fn v2_duration_premium_scales_per_month() {
        let cost = estimate_cost_v2(&CostInputs {
            expected_duration_months: 24,
            ..baseline_inputs("us-federal", 0)
        });
        // 75,000 × (1 + 0.05 × 12) = 75,000 × 1.60 = 120,000
        assert_eq!(cost, d("120000"));
    }

    /// Duration is clamped at 60 months.
    #[test]
    fn v2_duration_clamped_at_cap() {
        let at_cap = estimate_cost_v2(&CostInputs {
            expected_duration_months: 60,
            ..baseline_inputs("us-federal", 0)
        });
        let over_cap = estimate_cost_v2(&CostInputs {
            expected_duration_months: 999,
            ..baseline_inputs("us-federal", 0)
        });
        // 75,000 × (1 + 0.05 × 48) = 75,000 × 3.40 = 255,000
        assert_eq!(at_cap, d("255000"));
        assert_eq!(at_cap, over_cap);
    }

    // ── S6.7 v2: party factor ───────────────────────────────────────────────

    /// Party count at or below the 2-party baseline adds no premium.
    #[test]
    fn v2_party_at_baseline_is_neutral() {
        let at = estimate_cost_v2(&CostInputs {
            party_count: BASELINE_PARTY_COUNT,
            ..baseline_inputs("us-state", 0)
        });
        let below = estimate_cost_v2(&CostInputs {
            party_count: 1,
            ..baseline_inputs("us-state", 0)
        });
        assert_eq!(at, d("35000"));
        assert_eq!(below, d("35000"));
    }

    /// 5 parties → 3 over baseline × 15% = +45% → base × 1.45.
    #[test]
    fn v2_party_premium_scales_per_party() {
        let cost = estimate_cost_v2(&CostInputs {
            party_count: 5,
            ..baseline_inputs("us-state", 0)
        });
        // 35,000 × (1 + 0.15 × 3) = 35,000 × 1.45 = 50,750
        assert_eq!(cost, d("50750"));
    }

    /// Party count is clamped at 20.
    #[test]
    fn v2_party_clamped_at_cap() {
        let at_cap = estimate_cost_v2(&CostInputs {
            party_count: 20,
            ..baseline_inputs("us-state", 0)
        });
        let over_cap = estimate_cost_v2(&CostInputs {
            party_count: 500,
            ..baseline_inputs("us-state", 0)
        });
        // 35,000 × (1 + 0.15 × 18) = 35,000 × 3.70 = 129,500
        assert_eq!(at_cap, d("129500"));
        assert_eq!(at_cap, over_cap);
    }

    // ── S6.7 v2: factor composition ─────────────────────────────────────────

    /// All three factors compose multiplicatively.
    #[test]
    fn v2_all_factors_compose() {
        let cost = estimate_cost_v2(&CostInputs {
            jurisdiction: "us-federal",
            motion_count: 5,            // ×1.40
            expected_duration_months: 24, // ×1.60
            party_count: 5,             // ×1.45
        });
        // 75,000 × 1.40 × 1.60 × 1.45 = 243,600
        assert_eq!(cost, d("243600"));
    }

    /// estimate_cost (v1) == estimate_cost_v2 at both baselines, for a
    /// spread of jurisdictions and motion counts.
    #[test]
    fn v1_is_v2_at_baselines() {
        for jurisdiction in ["us-federal", "us-state", "ie-supreme", ""] {
            for motions in [0_u32, 1, 7, 50, 200] {
                assert_eq!(
                    estimate_cost(jurisdiction, motions),
                    estimate_cost_v2(&baseline_inputs(jurisdiction, motions)),
                    "v1/v2 mismatch for {jurisdiction:?} motions={motions}"
                );
            }
        }
    }

    // ── S6.7 v2: derive_duration_months helper ──────────────────────────────

    #[test]
    fn derive_duration_starts_at_baseline() {
        assert_eq!(derive_duration_months(0), BASELINE_DURATION_MONTHS);
    }

    #[test]
    fn derive_duration_adds_one_month_per_motion() {
        assert_eq!(derive_duration_months(6), 18);
        assert_eq!(derive_duration_months(30), 42);
    }

    #[test]
    fn derive_duration_clamped_at_cap() {
        assert_eq!(derive_duration_months(50), 60);
        assert_eq!(derive_duration_months(9_999), 60);
    }
}
