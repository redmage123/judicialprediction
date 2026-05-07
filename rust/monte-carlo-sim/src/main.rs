// FUNCTIONAL-CORE (binary shell only; simulation logic will live in lib.rs)
// Pure (seed, params) -> Trajectory closures; rayon par_iter over N seeds.

use rayon::prelude::*;

#[derive(Debug, Clone)]
pub struct SimParams {
    pub n_trials: usize,
    pub base_win_probability: f64,
}

/// Pure simulation: given a seed and params, return a win/loss indicator.
/// No I/O, no shared mutable state.
fn simulate_trial(seed: u64, params: &SimParams) -> bool {
    // Deterministic LCG: not for production, illustrates the pure-function shape.
    let pseudo_rand = (seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) >> 33) as f64
        / u32::MAX as f64;
    pseudo_rand < params.base_win_probability
}

fn run_simulation(params: &SimParams) -> f64 {
    let wins: u64 = (0..params.n_trials as u64)
        .into_par_iter()
        .filter(|&seed| simulate_trial(seed, params))
        .count() as u64;
    wins as f64 / params.n_trials as f64
}

fn main() {
    tracing_subscriber::fmt::init();

    let params = SimParams { n_trials: 10_000, base_win_probability: 0.6 };
    let win_rate = run_simulation(&params);
    tracing::info!(win_rate, "monte-carlo-sim placeholder complete");
    println!("Simulated win rate: {win_rate:.4}");
}
