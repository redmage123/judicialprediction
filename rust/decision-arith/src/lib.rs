// FUNCTIONAL-CORE
// Pure functions: EV, CVaR, Nash, Rubinstein, prospect-theory utility.
// No I/O, no mutable global state, no unsafe.

/// Expected value of a discrete probability distribution.
/// `outcomes`: slice of (probability, value) pairs; probabilities must sum to 1.
pub fn expected_value(outcomes: &[(f64, f64)]) -> f64 {
    outcomes.iter().map(|(p, v)| p * v).sum()
}

/// Conditional Value-at-Risk (CVaR / Expected Shortfall) at confidence level α.
/// `outcomes` must be sorted ascending by value.
/// CVaR(α) is the mean of the worst-α fraction of outcomes.
///
/// Edge-case note: a small epsilon (1e-12) is added to the alpha threshold so that
/// floating-point accumulated probabilities that should sum to alpha but land
/// marginally below it are still included in the tail. This preserves the
/// invariant cvar(outcomes, 1.0) == expected_value(outcomes).
pub fn cvar(outcomes: &[(f64, f64)], alpha: f64) -> f64 {
    debug_assert!((0.0..=1.0).contains(&alpha));
    // Epsilon guard: floating-point sums that theoretically equal alpha may fall
    // just below it. The guard is tight enough to not affect any alpha < 1 - 1e-11.
    let threshold = alpha + 1e-12;
    let mut cumulative = 0.0;
    let mut tail_ev = 0.0;
    let mut tail_p = 0.0;
    for &(p, v) in outcomes {
        if cumulative + p <= threshold {
            tail_ev += p * v;
            tail_p += p;
        }
        cumulative += p;
    }
    if tail_p > 0.0 { tail_ev / tail_p } else { 0.0 }
}

/// Symmetric Nash bargaining solution.
///
/// Given disagreement payoffs (`d_a`, `d_b`) and a total `surplus` to split,
/// the symmetric Nash solution maximises the product of gains:
///   (x - d_a)(y - d_b) subject to (x - d_a) + (y - d_b) = surplus.
///
/// The unique solution is each party receives half the surplus above their
/// disagreement payoff: (d_a + surplus/2, d_b + surplus/2).
///
/// Properties guaranteed:
/// - Individual rationality: both parties receive ≥ their disagreement payoff.
/// - Pareto efficiency: total gains equal `surplus` (no surplus left on the table).
/// - Symmetry: equal surplus → equal payoffs above disagreement point.
pub fn nash_bargaining(d_a: f64, d_b: f64, surplus: f64) -> (f64, f64) {
    let half = surplus.max(0.0) / 2.0;
    (d_a + half, d_b + half)
}

/// Placeholder: Rubinstein alternating-offers solution (Sprint 2).
pub fn rubinstein_offer(_delta_a: f64, _delta_b: f64, _pie: f64) -> f64 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ev_coin_flip() {
        let outcomes = [(0.5, 10.0), (0.5, 0.0)];
        assert!((expected_value(&outcomes) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn ev_certain() {
        let outcomes = [(1.0, 42.0)];
        assert!((expected_value(&outcomes) - 42.0).abs() < 1e-10);
    }

    #[test]
    fn cvar_full_alpha_equals_mean() {
        let outcomes = [(0.3, -10.0), (0.4, 5.0), (0.3, 20.0)];
        let mean = expected_value(&outcomes);
        let cv = cvar(&outcomes, 1.0);
        assert!((cv - mean).abs() < 1e-9, "cvar(1.0)={cv} != mean={mean}");
    }

    #[test]
    fn nash_splits_surplus_evenly() {
        let (a, b) = nash_bargaining(10.0, 20.0, 40.0);
        assert!((a - 30.0).abs() < 1e-10, "a={a}");
        assert!((b - 40.0).abs() < 1e-10, "b={b}");
    }
}
