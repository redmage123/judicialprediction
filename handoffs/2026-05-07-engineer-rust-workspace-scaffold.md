# Handoff — Rust Workspace Scaffold (S1.10)

**From:** gigforge-engineer (Chris Novak)
**To:** PM / next engineer
**Date:** 2026-05-07
**Story:** S1.10 — Scaffold Rust workspace
**Plane issue:** JP-3

---

## Status: COMPLETE — cargo check passes

All 10 crates compile cleanly.

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.33s
```

---

## Files Created

### Workspace root
- `rust/Cargo.toml` — workspace members (10 crates), resolver=2, release opt-level=3, shared `[workspace.dependencies]`

### Crates

| Crate | Type | ADR-FP-001 tier | Entry point |
|-------|------|-----------------|-------------|
| `api-gateway` | binary | Tier 2 (functional-leaning) | src/main.rs — axum health route on :4000 |
| `feature-store-types` | library | **Tier 1 (FUNCTIONAL-CORE)** | src/lib.rs — `Tier`, `Sensitivity`, `PermittedUse`, `TieredFeature<T>` ADTs + unit tests |
| `feature-store` | library | Tier 3 (imperative shell) | src/lib.rs — re-exports types, placeholder `FeatureStoreRepo` |
| `decision-arith` | library | **Tier 1 (FUNCTIONAL-CORE)** | src/lib.rs — `expected_value`, `cvar`, placeholder `nash_bargaining`, `rubinstein_offer` + unit tests |
| `monte-carlo-sim` | binary | **Tier 1 (FUNCTIONAL-CORE)** | src/main.rs — rayon par_iter over pure `simulate_trial(seed, params)` closures |
| `cost-engine` | library | **Tier 1 (FUNCTIONAL-CORE)** | src/lib.rs — `CostDistribution`, `compose_independent` + unit tests |
| `ingest-fetcher` | binary | Tier 3 (imperative) | src/main.rs — tracing init placeholder |
| `feature-deriver` | binary | Tier 2 (functional-leaning) | src/main.rs — rayon par_iter derivation placeholder |
| `event-broker` | binary | Tier 3 (imperative) | src/main.rs — tracing init placeholder |
| `partner-gateway` | binary | Tier 2 (functional-leaning) | src/main.rs — axum health route on :4001 |

---

## Toolchain

Rust 1.95.0 installed via rustup to `~/.cargo/`. Source `$HOME/.cargo/env` before any `cargo` invocation in a new shell.

System deps installed: `pkg-config`, `libssl-dev` (required by `openssl-sys` → `reqwest`).

---

## What the next person needs to know

1. **`// FUNCTIONAL-CORE` marker is in place** on all Tier-1 lib.rs files. The CI linter rule (ruff/clippy config) that enforces no mutable global state in marked files is not yet wired — that is a Sprint 2 CI task.

2. **Dependency versions are unpinned** (workspace uses `version = "X"` ranges). Sprint 2 task: run `cargo update`, snapshot `Cargo.lock`, and pin all workspace deps to exact versions in `[workspace.dependencies]`.

3. **No protos/ directory yet.** ADR-002 requires a `protos/` root with buf tooling. That is a separate Sprint 1 story.

4. **sqlx offline mode not configured.** `feature-store` depends on sqlx but has no `.sqlx/` query cache. `cargo check` passes because no `sqlx::query!` macros are used yet. Sprint 2 schema migration story will add these.

5. **`feature-store-types` tests pass inline.** Run `cargo test -p feature-store-types` to confirm the Tier-C gating logic works.

6. **No Docker / CI wiring yet.** The `rust/` workspace should be built in a `rust:1.95` base image. Dockerfile scaffold is a DevOps story.

---

## Verification

```
cargo check --workspace   # 0 errors, 0 warnings critical
cargo test -p feature-store-types -p decision-arith -p cost-engine  # all unit tests pass
```
