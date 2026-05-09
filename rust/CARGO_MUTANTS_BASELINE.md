# JudicialPredict — cargo-mutants Baseline

This file records mutation-testing survival counts for the four functional-core crates.
It is updated automatically by `scripts/mutants-weekly.sh` (every Monday 06:00 UTC).
Manual updates: run `cargo mutants -p <crate> --no-shuffle --timeout 1800` then edit this table.

## Baseline Summary

| Crate | Caught | Missed | Unviable | Total | Survival rate | Status |
|-------|--------|--------|----------|-------|---------------|--------|
| rate-limit | 13 | 0 | 1 | 14 | 0% survived | ✅ Baseline confirmed |
| decision-arith | 56 | 7 | 1 | 64 | 11% survived | ⚠️ Documented survivors |
| monte-carlo-sim | 19 | 8 | 0 | 27 | 30% survived | ⚠️ Documented survivors |
| feature-deriver | 8 | 1 | 0 | 9 | 11% survived | ⚠️ Documented survivors |
| **total** | **96** | **16** | **2** | **114** | **14% survived** | — |

_First sweep date: rate-limit 2026-05-09 (S2.8); decision-arith / monte-carlo-sim / feature-deriver 2026-05-09 (S3.12)._

---

## rate-limit — Baseline Detail (2026-05-09)

**Crate:** `rust/rate-limit`
**Source:** `src/lib.rs` (single pure function `check`)
**cargo-mutants version:** 27.0.0
**Run date:** 2026-05-09
**Timeout per mutant:** 300s

### Mutation outcomes

| Outcome | Count |
|---------|-------|
| Caught | 13 |
| Missed | 0 |
| Unviable | 1 |
| **Total** | **14** |

### Caught mutations (all 13)

All mutations in the `check` function were caught by the proptest suite:

| Line | Mutation | Caught by |
|------|----------|-----------|
| 63:35 | `+ → -` in `bucket.tokens + elapsed * ...` | `prop_refill_proportional_to_elapsed` |
| 63:35 | `+ → *` in `bucket.tokens + elapsed * ...` | `prop_refill_proportional_to_elapsed` |
| 63:45 | `* → +` in `elapsed * bucket.refill_per_sec` | `prop_refill_proportional_to_elapsed` |
| 63:45 | `* → /` in `elapsed * bucket.refill_per_sec` | `prop_refill_proportional_to_elapsed` |
| 68:22 | `>= → <` in token-sufficiency check | `full_bucket_allows_first_request` |
| 69:23 | `-= → +=` in token consumption | `tokens_decrease_by_cost_on_allow` |
| 69:23 | `-= → /=` in token consumption | `tokens_decrease_by_cost_on_allow` |
| 73:30 | `- → +` in `deficit = cost_f - tokens` | `prop_retry_after_ms_formula_correct` |
| 73:30 | `- → /` in `deficit = cost_f - tokens` | `prop_retry_after_ms_formula_correct` |
| 74:33 | `/ → %` in `deficit / refill_per_sec` | `prop_retry_after_ms_formula_correct` |
| 74:33 | `/ → *` in `deficit / refill_per_sec` | `prop_retry_after_ms_formula_correct` |
| 75:41 | `* → +` in `wait_secs * 1000.0` | `prop_retry_after_ms_formula_correct` |
| 75:41 | `* → /` in `wait_secs * 1000.0` | `prop_retry_after_ms_formula_correct` |

### Unviable mutations (1)

| Line | Mutation | Reason |
|------|----------|--------|
| 62:5 | `replace check -> Decision with Default::default()` | `Decision` does not impl `Default` — fails to compile (expected; not a coverage gap) |

### Surviving mutations to address

**rate-limit: 0 surviving — baseline confirmed.**

No surviving mutations. All mutable operators in the hot path are pinned by dedicated property tests.

---

## decision-arith — Baseline Detail (2026-05-09, S3.12)

**Run:** 64 mutants, 51 s wall-clock, timeout 300 s.
**Outcomes:** 56 caught, 7 missed, 1 unviable.

### Documented survivors (7)

| File:line | Mutation | Status | Rationale |
|-----------|----------|--------|-----------|
| `src/lib.rs:61:5` | `rubinstein_offer -> 1.0` | Documented | Function returns a single ratio; constant-replacement only fails on a future caller that asserts on the magnitude. Sprint-4 follow-up: add a property test that pins `rubinstein_offer(δ_a, δ_b) ∈ (0, 1)` for valid inputs. |
| `src/lib.rs:61:5` | `rubinstein_offer -> -1.0` | Documented | Same as above; negative output would be invalid but no test currently asserts non-negativity. Sprint-4: pin `>= 0`. |
| `src/recommend.rs:104:29` | `EV_settle > EV_try → >=` | Documented | Boundary case (exact equality) was not pinned; the existing `settle_when_ev_settle_dominates_and_low_ci` test uses strict-greater. Sprint-4: add an EV-tie test. |
| `src/recommend.rs:104:56` | `ci_lower < 0.40 → <=` | Documented | Boundary at 0.40 not pinned (no test uses `ci_lower == 0.40`). Sprint-4: add boundary tests. |
| `src/recommend.rs:106:34` | `&& → \|\|` in Try-rule | Documented | The Try rule combines two AND'd conditions; flipping to OR widens the rule but no test exercises only one of the conjuncts being true. Sprint-4: add a test where exactly one half is true. |
| `src/recommend.rs:106:22` | `EV_try > EV_settle → >=` | Documented | Same boundary class as line 104:29. |
| `src/recommend.rs:106:52` | `ci_lower > 0.55 → >=` | Documented | Same boundary class as line 104:56 but for the Try-rule threshold. |

**All 7 are boundary/equality and constant-substitution mutations.** None point at production-relevant gaps; they're tightening opportunities for Sprint 4.

## monte-carlo-sim — Baseline Detail (2026-05-09, S3.12)

**Run:** 27 mutants, 27 s wall-clock, timeout 300 s.
**Outcomes:** 19 caught, 8 missed, 0 unviable.

### Documented survivors (8)

Six are bit-twiddling mutations inside `splitmix64` PRNG (lines 21-23):

| File:line | Mutation | Rationale |
|-----------|----------|-----------|
| `src/lib.rs:21:17` | `>> → <<` in splitmix64 | The mutated PRNG still produces a deterministic stream; tests pin behaviour against expected outputs derived from a seed, so the substitute stream is "wrong but consistent". Sprint-4: add a known-good vector test (sample N=10 outputs from a fixed seed against a reference implementation). |
| `src/lib.rs:22:12` | `^ → \|` in splitmix64 | Same — output stream changes but tests don't assert against a reference vector. |
| `src/lib.rs:22:12` | `^ → &` in splitmix64 | Same. |
| `src/lib.rs:22:17` | `>> → <<` in splitmix64 | Same. |
| `src/lib.rs:23:11` | `^ → \|` in splitmix64 | Same. |
| `src/lib.rs:23:16` | `>> → <<` in splitmix64 | Same. |
| `src/lib.rs:36:22` | `< → <=` in `simulate_trial` boundary | Boundary equality at the trial-loop terminator; no test uses an exact-equality `n_trials`. Sprint-4: pin loop boundary. |
| `src/main.rs:5:5` | `replace main with ()` | Binary entrypoint — no test coverage on `main()` itself. **Acceptable.** Replacing with `()` would produce a no-op binary that nothing currently tests. |

## feature-deriver — Baseline Detail (2026-05-09, S3.12)

**Run:** 9 mutants, 12 s wall-clock, timeout 300 s.
**Outcomes:** 8 caught, 1 missed, 0 unviable.

### Documented survivors (1)

| File:line | Mutation | Rationale |
|-----------|----------|-----------|
| `src/main.rs:7:5` | `replace main with ()` | Binary entrypoint — same as monte-carlo-sim. **Acceptable.** |

---

## Notes for Sprint 3

- If a crate's `missed` count goes UP in the weekly report, treat it as a regression — add a new proptest that catches the surviving mutation.
- If `missed` goes DOWN, update the baseline JSON and this file.
- Unviable count changes are informational only (compiler rejects the mutated form).
- For adding new proptests, follow the `proptest! {}` macro pattern already used in each crate's `src/lib.rs`.
