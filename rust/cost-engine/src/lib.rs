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

    /// Variance of the distribution: E[X²] - E[X]².
    pub fn variance(&self) -> f64 {
        let mean = self.expected_cost();
        let e_sq: f64 = self.probabilities
            .iter()
            .zip(self.costs.iter())
            .map(|(p, c)| p * c * c)
            .sum();
        // Clamp to zero to handle tiny negative values from floating-point cancellation.
        (e_sq - mean * mean).max(0.0)
    }
}

/// Compose component distributions assuming full independence.
/// Returns the expected total cost (sum of component means).
/// For Sprint 2: full covariance-matrix composition with correlation input.
pub fn compose_independent(components: &[CostDistribution]) -> f64 {
    components.iter().map(|d| d.expected_cost()).sum()
}

/// Variance of the composed independent distribution.
/// Under independence: Var[X+Y] = Var[X] + Var[Y].
pub fn compose_variance(components: &[CostDistribution]) -> f64 {
    components.iter().map(|d| d.variance()).sum()
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
    fn variance_single() {
        // Var = E[X^2] - E[X]^2 = 0.5*10000 + 0.5*40000 - 150^2 = 25000 - 22500 = 2500
        let d = CostDistribution::new(vec![0.5, 0.5], vec![100.0, 200.0]);
        assert!((d.variance() - 2500.0).abs() < 1e-8, "variance={}", d.variance());
    }

    #[test]
    fn compose_two_independent() {
        let a = CostDistribution::new(vec![1.0], vec![50.0]);
        let b = CostDistribution::new(vec![1.0], vec![75.0]);
        assert!((compose_independent(&[a, b]) - 125.0).abs() < 1e-10);
    }

    #[test]
    fn compose_variance_two_independent() {
        let a = CostDistribution::new(vec![0.5, 0.5], vec![0.0, 100.0]);
        let b = CostDistribution::new(vec![0.5, 0.5], vec![0.0, 200.0]);
        let total_var = compose_variance(&[a.clone(), b.clone()]);
        assert!((total_var - (a.variance() + b.variance())).abs() < 1e-8);
    }
}
