// FUNCTIONAL-CORE (library portion)
// Pure (seed, params) -> bool closures; rayon par_iter aggregation.
// No I/O, no shared mutable state, no unsafe.

use rayon::prelude::*;

/// Parameters for a single simulation run.
#[derive(Debug, Clone)]
pub struct SimParams {
    pub n_trials: usize,
    pub base_win_probability: f64,
}

/// Pure, deterministic trial: given a 64-bit seed and params, return win/loss.
///
/// Uses a one-step LCG transform (Knuth multiplicative constants) to derive a
/// pseudo-random value in [0, 1). Same seed + params always produces the same
/// output — this is the functional-core determinism guarantee tested by proptest.
pub fn simulate_trial(seed: u64, params: &SimParams) -> bool {
    let pseudo_rand = (seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
        >> 33) as f64
        / u32::MAX as f64;
    pseudo_rand < params.base_win_probability
}

/// Run N independent trials (seeds 0..n_trials) in parallel; return empirical win rate.
pub fn run_simulation(params: &SimParams) -> f64 {
    let wins: u64 = (0..params.n_trials as u64)
        .into_par_iter()
        .filter(|&seed| simulate_trial(seed, params))
        .count() as u64;
    wins as f64 / params.n_trials as f64
}
