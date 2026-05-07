// Property-based tests for cost-engine (ADR-FP-001 mandate).
// 256 test cases per property (proptest default).

use cost_engine::{compose_independent, compose_variance, CostDistribution};
use proptest::prelude::*;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Generate a valid two-outcome CostDistribution from a probability and two costs.
fn two_outcome(p: f64, c1: f64, c2: f64) -> CostDistribution {
    CostDistribution::new(vec![p, 1.0 - p], vec![c1, c2])
}

// ── Mean properties ──────────────────────────────────────────────────────────

proptest! {
    /// Mean of composed independent distributions equals sum of component means.
    #[test]
    fn compose_mean_equals_sum_of_means(
        p1 in 0.001_f64..0.999_f64,
        c1a in -100.0_f64..100.0_f64,
        c1b in -100.0_f64..100.0_f64,
        p2 in 0.001_f64..0.999_f64,
        c2a in -100.0_f64..100.0_f64,
        c2b in -100.0_f64..100.0_f64,
    ) {
        let a = two_outcome(p1, c1a, c1b);
        let b = two_outcome(p2, c2a, c2b);
        let expected_sum = a.expected_cost() + b.expected_cost();
        let composed     = compose_independent(&[a, b]);
        prop_assert!(
            (composed - expected_sum).abs() < 1e-9,
            "compose mean={composed}, sum of means={expected_sum}",
        );
    }

    /// Variance of composed independent distributions equals sum of component variances.
    /// Under independence: Var[X + Y] = Var[X] + Var[Y].
    #[test]
    fn compose_variance_equals_sum_of_variances(
        p1  in 0.001_f64..0.999_f64,
        c1a in -100.0_f64..100.0_f64,
        c1b in -100.0_f64..100.0_f64,
        p2  in 0.001_f64..0.999_f64,
        c2a in -100.0_f64..100.0_f64,
        c2b in -100.0_f64..100.0_f64,
    ) {
        let a = two_outcome(p1, c1a, c1b);
        let b = two_outcome(p2, c2a, c2b);
        let expected_var = a.variance() + b.variance();
        let composed_var  = compose_variance(&[a, b]);
        prop_assert!(
            (composed_var - expected_var).abs() < 1e-9,
            "compose variance={composed_var}, sum of variances={expected_var}",
        );
    }

    /// compose_independent is associative: compose([a,b,c]) == compose([a]) + compose([b,c]).
    /// (Equivalently: total mean = mean_a + (mean_b + mean_c) = (mean_a + mean_b) + mean_c.)
    #[test]
    fn compose_associative(
        p1  in 0.001_f64..0.999_f64,
        c1a in -100.0_f64..100.0_f64,
        c1b in -100.0_f64..100.0_f64,
        p2  in 0.001_f64..0.999_f64,
        c2a in -100.0_f64..100.0_f64,
        c2b in -100.0_f64..100.0_f64,
        p3  in 0.001_f64..0.999_f64,
        c3a in -100.0_f64..100.0_f64,
        c3b in -100.0_f64..100.0_f64,
    ) {
        let a = two_outcome(p1, c1a, c1b);
        let b = two_outcome(p2, c2a, c2b);
        let c = two_outcome(p3, c3a, c3b);
        // compose([a, b, c]) vs compose([a]) + compose([b, c])
        let full = compose_independent(&[a.clone(), b.clone(), c.clone()]);
        let left_assoc =
            compose_independent(&[a.clone()]) + compose_independent(&[b.clone(), c.clone()]);
        let right_assoc =
            compose_independent(&[a.clone(), b.clone()]) + compose_independent(&[c.clone()]);
        prop_assert!((full - left_assoc).abs() < 1e-9, "not left-associative");
        prop_assert!((full - right_assoc).abs() < 1e-9, "not right-associative");
    }

    /// compose_independent is commutative: compose([a, b]) == compose([b, a]).
    #[test]
    fn compose_commutative(
        p1  in 0.001_f64..0.999_f64,
        c1a in -100.0_f64..100.0_f64,
        c1b in -100.0_f64..100.0_f64,
        p2  in 0.001_f64..0.999_f64,
        c2a in -100.0_f64..100.0_f64,
        c2b in -100.0_f64..100.0_f64,
    ) {
        let a = two_outcome(p1, c1a, c1b);
        let b = two_outcome(p2, c2a, c2b);
        let ab = compose_independent(&[a.clone(), b.clone()]);
        let ba = compose_independent(&[b, a]);
        prop_assert!(
            (ab - ba).abs() < 1e-9,
            "compose not commutative: ab={ab}, ba={ba}",
        );
    }
}
