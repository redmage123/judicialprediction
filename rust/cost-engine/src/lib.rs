// FUNCTIONAL-CORE
// Distributional cost composition: (component-distributions, correlation-matrix) -> total-distribution.
// No I/O, no mutable global state, no unsafe.

/// A simple discrete cost distribution: parallel slices of probabilities and costs.
#[derive(Debug, Clone)]
pub struct CostDistribution {
    pub probabilities: Vec<f64>,
    pub costs: Vec<f64>,
}

impl CostDistribution {
    pub fn new(probabilities: Vec<f64>, costs: Vec<f64>) -> Self {
        debug_assert_eq!(probabilities.len(), costs.len());
        Self { probabilities, costs }
    }

    pub fn expected_cost(&self) -> f64 {
        self.probabilities
            .iter()
            .zip(self.costs.iter())
            .map(|(p, c)| p * c)
            .sum()
    }
}

/// Placeholder: compose component distributions under independence assumption.
/// Full covariance-matrix composition in Sprint 2.
pub fn compose_independent(components: &[CostDistribution]) -> f64 {
    components.iter().map(|d| d.expected_cost()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_cost_single() {
        let d = CostDistribution::new(vec![0.5, 0.5], vec![100.0, 200.0]);
        assert!((d.expected_cost() - 150.0).abs() < 1e-10);
    }

    #[test]
    fn compose_two_independent() {
        let a = CostDistribution::new(vec![1.0], vec![50.0]);
        let b = CostDistribution::new(vec![1.0], vec![75.0]);
        assert!((compose_independent(&[a, b]) - 125.0).abs() < 1e-10);
    }
}
