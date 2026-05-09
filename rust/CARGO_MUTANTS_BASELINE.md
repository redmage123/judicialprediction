# JudicialPredict — cargo-mutants Baseline

This file records mutation-testing survival counts for the four functional-core crates.
It is updated automatically by `scripts/mutants-weekly.sh` (every Monday 06:00 UTC).
Manual updates: run `cargo mutants -p <crate> --no-shuffle --timeout 1800` then edit this table.

## Baseline Summary

| Crate | Caught | Missed | Unviable | Total | Survival rate | Status |
|-------|--------|--------|----------|-------|---------------|--------|
| rate-limit | 13 | 0 | 1 | 14 | 0% survived | ✅ Baseline confirmed |
| decision-arith | TBD | TBD | TBD | TBD | — | ⏳ First cron run |
| monte-carlo-sim | TBD | TBD | TBD | TBD | — | ⏳ First cron run |
| feature-deriver | TBD | TBD | TBD | TBD | — | ⏳ First cron run |

_First cron run: Monday 2026-05-11 at 06:00 UTC will populate the TBD rows._

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

## decision-arith — Baseline Detail

_TBD — will be populated by the first weekly cron run (Monday 2026-05-11 06:00 UTC)._

## monte-carlo-sim — Baseline Detail

_TBD — will be populated by the first weekly cron run (Monday 2026-05-11 06:00 UTC)._

## feature-deriver — Baseline Detail

_TBD — will be populated by the first weekly cron run (Monday 2026-05-11 06:00 UTC)._

---

## Notes for Sprint 3

- If a crate's `missed` count goes UP in the weekly report, treat it as a regression — add a new proptest that catches the surviving mutation.
- If `missed` goes DOWN, update the baseline JSON and this file.
- Unviable count changes are informational only (compiler rejects the mutated form).
- For adding new proptests, follow the `proptest! {}` macro pattern already used in each crate's `src/lib.rs`.
