// Property-based tests for feature-store-types (ADR-FP-001 mandate).
// 256 test cases per property (proptest default).

use feature_store_types::{Sensitivity, Tier, TieredFeature};
use proptest::prelude::*;

// ── Tier strategy ─────────────────────────────────────────────────────────────

fn any_tier() -> impl Strategy<Value = Tier> {
    prop_oneof![
        Just(Tier::A),
        Just(Tier::B),
        Just(Tier::C),
        Just(Tier::D),
    ]
}

fn any_sensitivity() -> impl Strategy<Value = Sensitivity> {
    prop_oneof![
        Just(Sensitivity::Public),
        Just(Sensitivity::QuasiPublic),
        Just(Sensitivity::Inferred),
        Just(Sensitivity::Protected),
    ]
}

// ── Serde round-trip ──────────────────────────────────────────────────────────

proptest! {
    /// Every Tier value round-trips through JSON serialisation without loss of identity.
    #[test]
    fn tier_serde_roundtrip(tier in any_tier()) {
        let json  = serde_json::to_string(&tier).expect("ser failed");
        let back: Tier = serde_json::from_str(&json).expect("de failed");
        prop_assert_eq!(tier, back, "round-trip failed for {:?}", tier);
    }

    /// Every Sensitivity value round-trips through JSON without loss of identity.
    #[test]
    fn sensitivity_serde_roundtrip(sens in any_sensitivity()) {
        let json  = serde_json::to_string(&sens).expect("ser failed");
        let back: Sensitivity = serde_json::from_str(&json).expect("de failed");
        prop_assert_eq!(sens, back, "round-trip failed for {:?}", sens);
    }
}

// ── Tier-C model exclusion ────────────────────────────────────────────────────

proptest! {
    /// Tier-C features must not be readable without a PermittedUse.
    /// This enforces the runtime equivalent of a "PermittedUseInModel" bound:
    /// code that does not supply a PermittedUse cannot access the value.
    #[test]
    fn tier_c_gated_without_permitted_use(value in any::<i64>()) {
        let f = TieredFeature::new(value, Tier::C, Sensitivity::Protected);
        prop_assert!(f.read(None).is_none(), "Tier-C must require PermittedUse");
        prop_assert!(!f.is_safe_for_model(), "Tier-C must not be safe for model");
    }

    /// Non-Tier-C features are always readable without PermittedUse and safe for models.
    #[test]
    fn non_tier_c_always_readable(
        tier  in prop_oneof![Just(Tier::A), Just(Tier::B), Just(Tier::D)],
        sens  in any_sensitivity(),
        value in any::<i64>(),
    ) {
        let f = TieredFeature::new(value, tier, sens);
        prop_assert!(f.read(None).is_some(), "non-C tier must be readable without PermittedUse");
        prop_assert!(f.is_safe_for_model(), "non-C tier must be safe for model");
    }
}

// ── Model-safety ordering ─────────────────────────────────────────────────────

proptest! {
    /// Tier::C always returns None from model_safety_level; all others return Some.
    #[test]
    fn tier_c_has_no_model_safety_level(tier in any_tier()) {
        match tier {
            Tier::C => prop_assert!(tier.model_safety_level().is_none()),
            _       => prop_assert!(tier.model_safety_level().is_some()),
        }
    }

    /// The model-safety ordering A < B < D holds for every combination of safe tiers.
    #[test]
    fn model_safety_ordering_a_lt_b_lt_d(
        t1 in prop_oneof![Just(Tier::A), Just(Tier::B), Just(Tier::D)],
        t2 in prop_oneof![Just(Tier::A), Just(Tier::B), Just(Tier::D)],
    ) {
        // If t1 is "less safe" than t2, its level must be lower.
        // Concretely: level(A)=1, level(B)=2, level(D)=3.
        let l1 = t1.model_safety_level().unwrap();
        let l2 = t2.model_safety_level().unwrap();
        // Property: the level is a consistent total order — reflexivity and
        // the specific ordering A < B < D.
        if t1 == t2 {
            prop_assert_eq!(l1, l2);
        } else {
            // Check the established ordering constraints hold:
            // A < B, A < D, B < D.
            match (t1, t2) {
                (Tier::A, Tier::B) | (Tier::A, Tier::D) | (Tier::B, Tier::D) => {
                    prop_assert!(l1 < l2, "{t1:?}({l1}) should be < {t2:?}({l2})");
                }
                (Tier::B, Tier::A) | (Tier::D, Tier::A) | (Tier::D, Tier::B) => {
                    prop_assert!(l1 > l2, "{t1:?}({l1}) should be > {t2:?}({l2})");
                }
                _ => {} // same tier handled above
            }
        }
    }
}
