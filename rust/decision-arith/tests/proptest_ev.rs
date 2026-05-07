// Property-based tests for decision-arith (ADR-FP-001 mandate).
// 256 test cases per property (proptest default).

use decision_arith::{cvar, expected_value, nash_bargaining};
use proptest::prelude::*;

// ── Expected-value algebraic properties ─────────────────────────────────────

proptest! {
    /// EV with p_win=0 must equal the loss amount regardless of win amount.
    #[test]
    fn ev_zero_win_prob(
        win  in -1e6f64..1e6f64,
        lose in -1e6f64..1e6f64,
    ) {
        let outcomes = [(0.0_f64, win), (1.0, lose)];
        prop_assert!(
            (expected_value(&outcomes) - lose).abs() < 1e-9,
            "EV(p=0)={}, expected lose={}",
            expected_value(&outcomes), lose,
        );
    }

    /// EV with p_win=1 must equal the win amount regardless of loss amount.
    #[test]
    fn ev_certain_win(
        win  in -1e6f64..1e6f64,
        lose in -1e6f64..1e6f64,
    ) {
        let outcomes = [(1.0_f64, win), (0.0, lose)];
        prop_assert!(
            (expected_value(&outcomes) - win).abs() < 1e-9,
            "EV(p=1)={}, expected win={}",
            expected_value(&outcomes), win,
        );
    }

    /// Scale invariance: EV(k·win, k·lose) = k · EV(win, lose) for k > 0.
    #[test]
    fn ev_scale_invariance(
        p   in 0.001_f64..0.999_f64,
        win in -100.0_f64..100.0_f64,
        lose in -100.0_f64..100.0_f64,
        k   in 0.001_f64..100.0_f64,
    ) {
        let base    = [(p, win), (1.0 - p, lose)];
        let scaled  = [(p, k * win), (1.0 - p, k * lose)];
        let diff = (expected_value(&scaled) - k * expected_value(&base)).abs();
        prop_assert!(diff < 1e-6, "scale invariance broken: diff={diff}");
    }

    /// Translation invariance: EV(win+c, lose+c) = EV(win, lose) + c.
    #[test]
    fn ev_translation_invariance(
        p    in 0.001_f64..0.999_f64,
        win  in -100.0_f64..100.0_f64,
        lose in -100.0_f64..100.0_f64,
        c    in -100.0_f64..100.0_f64,
    ) {
        let base    = [(p, win), (1.0 - p, lose)];
        let shifted = [(p, win + c), (1.0 - p, lose + c)];
        let diff = (expected_value(&shifted) - (expected_value(&base) + c)).abs();
        prop_assert!(diff < 1e-6, "translation invariance broken: diff={diff}");
    }
}

// ── CVaR algebraic properties ────────────────────────────────────────────────

proptest! {
    /// CVaR(α=1.0) must equal the full distribution mean.
    /// Tests the floating-point epsilon guard added to cvar().
    #[test]
    fn cvar_at_1_equals_mean(
        p  in 0.001_f64..0.999_f64,
        v1 in -100.0_f64..100.0_f64,
        v2 in -100.0_f64..100.0_f64,
    ) {
        let (lo, hi) = if v1 <= v2 { (v1, v2) } else { (v2, v1) };
        let outcomes = [(p, lo), (1.0 - p, hi)];
        let mean = expected_value(&outcomes);
        let cv   = cvar(&outcomes, 1.0);
        prop_assert!(
            (cv - mean).abs() < 1e-6,
            "cvar(1.0)={cv} != mean={mean}",
        );
    }

    /// CVaR(α=p) over a two-outcome distribution (loss, gain) equals the loss value.
    /// The bottom-p tail contains exactly one outcome: loss.
    #[test]
    fn cvar_bottom_tail_equals_loss(
        p    in 0.05_f64..0.95_f64,
        loss in -100.0_f64..-0.001_f64,   // strictly negative
        gain in  0.001_f64..100.0_f64,    // strictly positive
    ) {
        // Sorted ascending: loss < 0 < gain.
        let outcomes = [(p, loss), (1.0 - p, gain)];
        let cv = cvar(&outcomes, p);
        prop_assert!(
            (cv - loss).abs() < 1e-6,
            "cvar(p={p})={cv}, expected loss={loss}",
        );
    }
}

// ── Nash bargaining properties ───────────────────────────────────────────────

proptest! {
    /// Individual rationality: neither party should receive less than their disagreement payoff.
    #[test]
    fn nash_individual_rationality(
        d_a     in -100.0_f64..100.0_f64,
        d_b     in -100.0_f64..100.0_f64,
        surplus in  0.0_f64..500.0_f64,
    ) {
        let (ra, rb) = nash_bargaining(d_a, d_b, surplus);
        prop_assert!(ra >= d_a - 1e-9, "a below disagreement: {ra} < {d_a}");
        prop_assert!(rb >= d_b - 1e-9, "b below disagreement: {rb} < {d_b}");
    }

    /// Pareto efficiency: total surplus claimed must equal the available surplus.
    /// Any unclaimed surplus is wasteful and ruled out by the Nash solution.
    #[test]
    fn nash_pareto_efficient(
        d_a     in -100.0_f64..100.0_f64,
        d_b     in -100.0_f64..100.0_f64,
        surplus in  0.001_f64..500.0_f64,
    ) {
        let (ra, rb) = nash_bargaining(d_a, d_b, surplus);
        let total_gain = (ra - d_a) + (rb - d_b);
        prop_assert!(
            (total_gain - surplus).abs() < 1e-9,
            "Pareto inefficient: total_gain={total_gain}, surplus={surplus}",
        );
    }
}
