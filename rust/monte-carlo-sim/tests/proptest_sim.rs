// Property-based tests for monte-carlo-sim (ADR-FP-001 mandate).
// 256 test cases per property (proptest default).

use monte_carlo_sim::{run_simulation, simulate_trial, SimParams};
use proptest::prelude::*;

// ── Determinism ──────────────────────────────────────────────────────────────

proptest! {
    /// simulate_trial is pure: the same (seed, params) always returns the same value.
    #[test]
    fn simulate_trial_deterministic(
        seed in 0u64..u64::MAX,
        p    in 0.001_f64..0.999_f64,
    ) {
        let params = SimParams { n_trials: 1, base_win_probability: p };
        let r1 = simulate_trial(seed, &params);
        let r2 = simulate_trial(seed, &params);
        prop_assert_eq!(r1, r2, "different results for same seed={} p={}", seed, p);
    }
}

// ── Seed independence (statistical) ──────────────────────────────────────────

proptest! {
    /// For any non-trivial win probability the LCG produces a mix of true and false
    /// across 256 consecutive seeds, confirming seeds are effectively independent.
    #[test]
    fn simulation_outcomes_vary_across_seeds(
        p in 0.05_f64..0.95_f64,
    ) {
        let params = SimParams { n_trials: 1, base_win_probability: p };
        let results: Vec<bool> = (0u64..256).map(|s| simulate_trial(s, &params)).collect();
        let some_true  = results.iter().any(|&r| r);
        let some_false = results.iter().any(|&r| !r);
        prop_assert!(some_true,  "all 256 seeds returned false for p={p}");
        prop_assert!(some_false, "all 256 seeds returned true  for p={p}");
    }
}

// ── Convergence to analytical EV ─────────────────────────────────────────────

proptest! {
    /// Aggregating 10 000 independent trials: empirical win rate must be within
    /// 5 % (absolute) of the theoretical win probability.
    ///
    /// With N=10 000 and p ∈ [0.1, 0.9], the standard deviation of the sample
    /// mean is at most sqrt(0.25/10000) ≈ 0.005.  A 5 % tolerance is 10σ — the
    /// probability of exceeding it under the true distribution is negligible.
    #[test]
    fn run_simulation_converges_to_base_probability(
        p in 0.1_f64..0.9_f64,
    ) {
        let params = SimParams { n_trials: 10_000, base_win_probability: p };
        let win_rate = run_simulation(&params);
        prop_assert!(
            (win_rate - p).abs() < 0.05,
            "win_rate={win_rate:.4} diverges from p={p:.4} by more than 5 %",
        );
    }
}
