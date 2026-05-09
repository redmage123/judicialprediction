//! Functional-core token-bucket rate limiter.
//!
//! All state mutation flows through the pure [`check`] function — no I/O,
//! no async, no global state. The imperative shell (DashMap store, Tower
//! middleware, gRPC store) lives in `api-gateway::rate_limit`.
//!
//! ADR-FP-001: functional-core / imperative-shell.

use std::time::Instant;

/// Token bucket state — pure data, no I/O.
///
/// Callers own this struct; the [`check`] function mutates it in place.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    /// Maximum number of tokens (ceiling for refill).
    pub capacity: u32,
    /// Tokens replenished per second (may be fractional).
    pub refill_per_sec: f64,
    /// Current available tokens (fractional for sub-second precision).
    pub tokens: f64,
    /// Instant at which tokens were last recalculated.
    pub last_refill: Instant,
}

impl TokenBucket {
    /// Create a new bucket at full capacity.
    ///
    /// `refill_per_sec` is typically `requests_per_min / 60.0`.
    pub fn new(capacity: u32, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            refill_per_sec,
            tokens: capacity as f64,
            last_refill: Instant::now(),
        }
    }
}

/// Outcome of a [`check`] call.
#[derive(Debug, PartialEq)]
pub enum Decision {
    /// Request is allowed; `cost` tokens have been consumed from the bucket.
    Allow,
    /// Request is denied; the bucket had fewer tokens than `cost`.
    Deny {
        /// Milliseconds the caller should wait before retrying.
        retry_after_ms: u64,
    },
}

/// Refill the bucket proportional to elapsed time, then try to consume `cost`
/// tokens.
///
/// This is a **pure function**: all state mutation is confined to `bucket`;
/// the caller provides `now` (no hidden clock reads).
///
/// # Postcondition
/// `bucket.tokens <= bucket.capacity as f64` after this call.
pub fn check(bucket: &mut TokenBucket, now: Instant, cost: u32) -> Decision {
    // Refill proportional to elapsed wall time, capped at capacity.
    let elapsed = now.saturating_duration_since(bucket.last_refill).as_secs_f64();
    let refilled = (bucket.tokens + elapsed * bucket.refill_per_sec).min(bucket.capacity as f64);
    bucket.tokens = refilled;
    bucket.last_refill = now;

    let cost_f = cost as f64;
    if bucket.tokens >= cost_f {
        bucket.tokens -= cost_f;
        Decision::Allow
    } else {
        // Compute how long until the deficit is replenished.
        let deficit = cost_f - bucket.tokens;
        let wait_secs = deficit / bucket.refill_per_sec;
        let retry_after_ms = (wait_secs * 1000.0).ceil() as u64;
        Decision::Deny { retry_after_ms }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use proptest::prelude::*;

    // --- Unit tests (6) ---

    #[test]
    fn full_bucket_allows_first_request() {
        let now = Instant::now();
        let mut b = TokenBucket { capacity: 10, refill_per_sec: 1.0, tokens: 10.0, last_refill: now };
        assert_eq!(check(&mut b, now, 1), Decision::Allow);
    }

    #[test]
    fn empty_bucket_denies() {
        let now = Instant::now();
        let mut b = TokenBucket { capacity: 5, refill_per_sec: 1.0, tokens: 0.0, last_refill: now };
        assert!(matches!(check(&mut b, now, 1), Decision::Deny { .. }), "empty bucket must deny");
    }

    #[test]
    fn deny_retry_after_ms_is_positive() {
        let now = Instant::now();
        // 0.5 tokens available, cost is 1 → deny.
        let mut b = TokenBucket { capacity: 10, refill_per_sec: 1.0, tokens: 0.5, last_refill: now };
        match check(&mut b, now, 1) {
            Decision::Deny { retry_after_ms } => assert!(retry_after_ms > 0),
            Decision::Allow => panic!("should have denied with only 0.5 tokens vs cost 1"),
        }
    }

    #[test]
    fn refill_does_not_exceed_capacity() {
        let now = Instant::now();
        let mut b = TokenBucket { capacity: 10, refill_per_sec: 5.0, tokens: 0.0, last_refill: now };
        // Advance 1 000 s — would overflow to 5 000 tokens without the cap.
        check(&mut b, now + Duration::from_secs(1_000), 0);
        assert!(
            b.tokens <= b.capacity as f64,
            "tokens {} must not exceed capacity {}",
            b.tokens, b.capacity
        );
    }

    #[test]
    fn tokens_decrease_by_cost_on_allow() {
        let now = Instant::now();
        let mut b = TokenBucket { capacity: 10, refill_per_sec: 1.0, tokens: 10.0, last_refill: now };
        check(&mut b, now, 3);
        assert!((b.tokens - 7.0).abs() < 1e-9, "expected 7.0 tokens after cost-3 consume");
    }

    #[test]
    fn refill_restores_tokens_over_time() {
        let now = Instant::now();
        // Start empty; 1 token/s refill; wait 30 s — should allow a cost-1 request.
        let mut b = TokenBucket { capacity: 60, refill_per_sec: 1.0, tokens: 0.0, last_refill: now };
        assert_eq!(
            check(&mut b, now + Duration::from_secs(30), 1),
            Decision::Allow,
            "should allow after 30 s of refill"
        );
    }

    // --- Property tests (2) ---

    proptest! {
        /// Refill never overshoots capacity, regardless of elapsed time or initial tokens.
        #[test]
        fn prop_refill_never_overshoots_capacity(
            capacity in 1u32..=200,
            elapsed_ms in 0u64..=100_000_000,
            initial_tokens_pct in 0.0f64..=1.0,
        ) {
            let cap_f = capacity as f64;
            let now = Instant::now();
            let mut b = TokenBucket {
                capacity,
                refill_per_sec: cap_f / 60.0,
                tokens: initial_tokens_pct * cap_f,
                last_refill: now,
            };
            let later = now + Duration::from_millis(elapsed_ms);
            check(&mut b, later, 0); // cost=0: pure refill, no consume
            prop_assert!(
                b.tokens <= cap_f + 1e-9,
                "tokens={} exceeds capacity={}",
                b.tokens, cap_f
            );
        }

        /// When the decision is Deny, the bucket must have had fewer tokens than cost
        /// at the moment of the check (same instant → no refill between observation and decision).
        #[test]
        fn prop_deny_implies_insufficient_tokens(
            capacity in 1u32..=100,
            tokens_pct in 0.0f64..=1.0,
            cost in 1u32..=200,
        ) {
            let cap_f = capacity as f64;
            let now = Instant::now();
            let initial_tokens = tokens_pct * cap_f;
            let mut b = TokenBucket {
                capacity,
                refill_per_sec: cap_f / 60.0,
                tokens: initial_tokens,
                last_refill: now, // same instant → elapsed = 0 → no refill
            };
            let decision = check(&mut b, now, cost);
            if matches!(decision, Decision::Deny { .. }) {
                prop_assert!(
                    initial_tokens < cost as f64,
                    "Deny issued but initial_tokens={initial_tokens} >= cost={cost}"
                );
            }
        }

        /// retry_after_ms matches the formula: ceil((cost - tokens) / rate * 1000).
        ///
        /// This pins all three arithmetic operators in the Deny path:
        ///   deficit    = cost - tokens        (not + or /)
        ///   wait_secs  = deficit / rate        (not % or *)
        ///   retry_ms   = ceil(wait_secs * 1000) (not + or /)
        ///
        /// We set tokens_below_cost = tokens strictly below cost so a Deny is
        /// guaranteed, and tokens > 0 so the `cost - tokens` vs `cost + tokens`
        /// mutation produces a different deficit (and thus different retry_after_ms).
        #[test]
        fn prop_retry_after_ms_formula_correct(
            capacity in 10u32..=1000,
            refill_per_sec in 0.1f64..=100.0,
            cost in 2u32..=500,
            // tokens is in [1, cost-1] so: tokens>0 AND tokens<cost → guaranteed Deny
            tokens_below in 1u32..=499u32,
        ) {
            let cost = cost.max(2);
            let tokens_f = (tokens_below % (cost - 1) + 1) as f64; // in [1, cost-1]
            let now = Instant::now();
            let mut b = TokenBucket {
                capacity,
                refill_per_sec,
                tokens: tokens_f,
                last_refill: now, // same instant → no refill
            };
            match check(&mut b, now, cost) {
                Decision::Deny { retry_after_ms } => {
                    // `check` caps tokens at capacity during refill; since elapsed=0
                    // and last_refill=now, the cap is the only thing that can change
                    // our initial tokens_f.  Mirror that here so the expected value
                    // matches what the implementation actually computed.
                    let effective_tokens = tokens_f.min(capacity as f64);
                    let deficit = cost as f64 - effective_tokens; // cost - effective_tokens (must be -)
                    let wait_secs = deficit / refill_per_sec;    // deficit / rate (must be /)
                    let expected_ms = (wait_secs * 1000.0).ceil() as u64; // * 1000 (must be *)
                    prop_assert_eq!(
                        retry_after_ms, expected_ms,
                        "retry_after_ms={} != expected ceil({}*1000)={} (effective_tokens={}, cost={}, deficit={}, rate={})",
                        retry_after_ms, wait_secs, expected_ms, effective_tokens, cost, deficit, refill_per_sec
                    );
                }
                Decision::Allow => {
                    prop_assert!(false,
                        "expected Deny for cost={cost} tokens={tokens_f} (tokens < cost)");
                }
            }
        }

        /// Refill amount is proportional to elapsed time × refill_per_sec.
        ///
        /// Pins the `*` operator in `elapsed * refill_per_sec` (line 63).
        /// The mutation `* → /` would make refill inversely proportional.
        #[test]
        fn prop_refill_proportional_to_elapsed(
            capacity in 10u32..=10_000,
            elapsed_ms in 1u64..=10_000,
            refill_per_sec in 0.5f64..=50.0,
        ) {
            let cap_f = capacity as f64;
            let now = Instant::now();
            // Start at 0 tokens so the refill shows up clearly.
            let mut b = TokenBucket {
                capacity,
                refill_per_sec,
                tokens: 0.0,
                last_refill: now,
            };
            let later = now + Duration::from_millis(elapsed_ms);
            check(&mut b, later, 0); // cost=0: observe refill without consuming
            let elapsed_secs = elapsed_ms as f64 / 1000.0;
            let expected_tokens = (elapsed_secs * refill_per_sec).min(cap_f);
            prop_assert!(
                (b.tokens - expected_tokens).abs() < 1e-6,
                "tokens={} expected {} (elapsed={}s rate={})",
                b.tokens, expected_tokens, elapsed_secs, refill_per_sec
            );
        }
    }
}
