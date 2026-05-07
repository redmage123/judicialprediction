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

/// Map a 64-bit seed to a uniform f64 in [0, 1) using the splitmix64 finalizer.
///
/// splitmix64 is a high-quality bijective hash: consecutive integer seeds
/// produce statistically independent-looking outputs, which is what makes
/// `run_simulation` converge to the analytical probability for seed ranges 0..N.
fn splitmix64(seed: u64) -> f64 {
    let mut x = seed.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x = x ^ (x >> 31);
    // Take the top 53 bits to get a uniform f64 in [0, 1).
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// Pure, deterministic trial: given a 64-bit seed and params, return win/loss.
///
/// Uses splitmix64 to derive a pseudo-random value in [0, 1) from the seed.
/// Same seed + params always produces the same output — this is the
/// functional-core determinism guarantee tested by proptest.
/// Consecutive seeds produce statistically independent outcomes, which ensures
/// `run_simulation` converges to `base_win_probability` for large N.
pub fn simulate_trial(seed: u64, params: &SimParams) -> bool {
    splitmix64(seed) < params.base_win_probability
}

/// Run N independent trials (seeds 0..n_trials) in parallel; return empirical win rate.
pub fn run_simulation(params: &SimParams) -> f64 {
    let wins: u64 = (0..params.n_trials as u64)
        .into_par_iter()
        .filter(|&seed| simulate_trial(seed, params))
        .count() as u64;
    wins as f64 / params.n_trials as f64
}
