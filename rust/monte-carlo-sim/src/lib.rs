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

/// One-step splitmix64 (Vigna): mix a 64-bit seed into a 64-bit state.
///
/// Implements the same finalizer as Sebastiano Vigna's reference
/// (`xoroshiro.di.unimi.it/splitmix64.c`).  Treating the seed as the initial
/// `x` accumulator and returning after a single `next()` call:
///
/// ```text
///   x = seed + 0x9e3779b97f4a7c15
///   x = (x ^ (x >> 30)) * 0xbf58476d1ce4e5b9
///   x = (x ^ (x >> 27)) * 0x94d049bb133111eb
///   return x ^ (x >> 31)
/// ```
///
/// Exposed `pub(crate)` so the known-vector tests can validate the u64
/// state directly (catches mutation of any of the magic constants or
/// shift counts that would otherwise still produce a plausible f64).
pub(crate) fn splitmix64_u64(seed: u64) -> u64 {
    let mut x = seed.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// Map a 64-bit seed to a uniform f64 in [0, 1) using the splitmix64 finalizer.
///
/// splitmix64 is a high-quality bijective hash: consecutive integer seeds
/// produce statistically independent-looking outputs, which is what makes
/// `run_simulation` converge to the analytical probability for seed ranges 0..N.
fn splitmix64(seed: u64) -> f64 {
    // Take the top 53 bits to get a uniform f64 in [0, 1).
    (splitmix64_u64(seed) >> 11) as f64 / (1u64 << 53) as f64
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

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Known-vector test (S5.12 / S3.12 follow-up): Sebastiano Vigna's reference
    /// splitmix64 produces a fixed sequence of u64 outputs for the seed range
    /// 0..N when started from `x = 0`.  Our `splitmix64_u64` is the single-step
    /// form, so calling it with `seed` is equivalent to Vigna's `next()`
    /// starting from `x = seed`.  These six vectors are the canonical values
    /// for seeds 0, 1, 2, 3, 42, and 0xdeadbeef — any drift in the magic
    /// constants (0x9e3779…, 0xbf58…, 0x94d0…) or the shift counts (30/27/31)
    /// will fail this assertion.
    #[test]
    fn splitmix64_u64_matches_vigna_reference_vectors() {
        const VECTORS: &[(u64, u64)] = &[
            (0,           0xe220a8397b1dcdaf),
            (1,           0x910a2dec89025cc1),
            (2,           0x975835de1c9756ce),
            (3,           0x1d0b14e4db018fed),
            (42,          0xbdd732262feb6e95),
            (0xdeadbeef,  0x4adfb90f68c9eb9b),
        ];
        for &(seed, expected) in VECTORS {
            let actual = splitmix64_u64(seed);
            assert_eq!(
                actual, expected,
                "splitmix64_u64({seed}) = 0x{actual:016x}, expected 0x{expected:016x}",
            );
        }
    }

    /// The f64 mapping `(x >> 11) / 2^53` must match the canonical [0,1)
    /// projection for the same seeds.  Catches any drift in the bit-shift
    /// or the divisor that would still leave the u64 step intact.
    #[test]
    fn splitmix64_f64_matches_top53_projection() {
        // (seed, expected f64) — computed via the Python reference in the
        // S5.12 commit message: (splitmix64_u64(seed) >> 11) / 2^53
        const VECTORS: &[(u64, f64)] = &[
            (0,    0.883_310_808_213_642_6),
            (1,    0.566_561_575_172_280_9),
            (2,    0.591_189_734_198_079_4),
            (3,    0.113_450_342_057_154_54),
            (42,   0.741_564_878_771_823_3),
        ];
        for &(seed, expected) in VECTORS {
            let actual = splitmix64(seed);
            // f64 from a deterministic integer op should match bit-for-bit.
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "splitmix64({seed}) = {actual}, expected {expected}",
            );
        }
    }

    /// Determinism: calling splitmix64 twice with the same seed returns the
    /// same f64 (the functional-core guarantee the rest of the sim relies on).
    #[test]
    fn splitmix64_is_deterministic_across_calls() {
        for seed in [0_u64, 1, 99, 12_345, u64::MAX] {
            assert_eq!(splitmix64(seed), splitmix64(seed));
            assert_eq!(splitmix64_u64(seed), splitmix64_u64(seed));
        }
    }

    /// Output range: every f64 from splitmix64 lies in [0, 1).  Catches a
    /// mutation of the `>> 11` shift to a smaller value that would let the
    /// projection exceed 1.0.
    #[test]
    fn splitmix64_f64_always_in_unit_interval() {
        // Sample a spread of seeds across the u64 range.
        for seed in [0, 1, 1_000, u64::MAX / 2, u64::MAX - 1, u64::MAX] {
            let f = splitmix64(seed);
            assert!(
                (0.0..1.0).contains(&f),
                "splitmix64({seed}) = {f} is outside [0, 1)",
            );
        }
    }
}
