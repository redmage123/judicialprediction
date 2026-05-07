// FUNCTIONAL-CORE (binary shell only — simulation logic lives in lib.rs)
use monte_carlo_sim::{run_simulation, SimParams};

fn main() {
    tracing_subscriber::fmt::init();

    let params = SimParams { n_trials: 10_000, base_win_probability: 0.6 };
    let win_rate = run_simulation(&params);
    tracing::info!(win_rate, "monte-carlo-sim placeholder complete");
    println!("Simulated win rate: {win_rate:.4}");
}
