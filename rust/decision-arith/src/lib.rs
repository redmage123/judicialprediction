// FUNCTIONAL-CORE
// Pure functions: EV, CVaR, Nash, Rubinstein, prospect-theory utility.
// No I/O, no mutable global state, no unsafe.

/// Expected value of a discrete probability distribution.
/// `outcomes`: slice of (probability, value) pairs; probabilities must sum to 1.
pub fn expected_value(outcomes: &[(f64, f64)]) -> f64 {
    outcomes.iter().map(|(p, v)| p * v).sum()
}

/// Conditional Value-at-Risk (CVaR / Expected Shortfall) at confidence level α.
/// `outcomes` sorted ascending by value.
pub fn cvar(outcomes: &[(f64, f64)], alpha: f64) -> f64 {
    debug_assert!((0.0..=1.0).contains(&alpha));
    let mut cumulative = 0.0;
    let mut tail_ev = 0.0;
    let mut tail_p = 0.0;
    for &(p, v) in outcomes {
        if cumulative + p <= alpha {
            tail_ev += p * v;
            tail_p += p;
        }
        cumulative += p;
    }
    if tail_p > 0.0 { tail_ev / tail_p } else { 0.0 }
}

/// Placeholder: Nash bargaining solution (Sprint 2 full implementation).
pub fn nash_bargaining(_d_a: f64, _d_b: f64, _surplus: f64) -> (f64, f64) {
    (0.0, 0.0)
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
}
