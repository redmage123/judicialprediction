# Handoff — S2.1: feature-store gRPC split

**From:** gigforge-engineer (Chris Novak)
**To:** gigforge-pm (Jamie Okafor), next sprint engineer
**Date:** 2026-05-07
**Plane issue:** JP-24

---

## What was built

### Already on disk from the previous dispatch
- `rust/feature-store/src/server.rs` — tonic `FeatureStoreService` impl wrapping the Repo
- `rust/feature-store/src/main.rs` — binary entrypoint, binds gRPC on `0.0.0.0:4001`
- `rust/feature-store/Cargo.toml` — `[[bin]] feature-store-server`, tonic deps added
- `rust/feature-store/src/lib.rs` — `pub mod server` export added

### Completed this dispatch
- `rust/api-gateway/src/app.rs` — already had the full tonic `FeatureStoreServiceClient`
  wiring: `Channel::from_shared(...).connect_lazy()`, tenant-id metadata on every call,
  structured GraphQL error on connection failure
- `rust/api-gateway/Cargo.toml` — already had `feature-store` path dep + `tonic` runtime dep
- `rust/api-gateway/tests/e2e_smoke.rs` — already had all 4 tests:
  - `graphql_feature_rls_smoke` (spawns feature-store in-process, probes RLS isolation)
  - `feature_store_grpc_unavailable_returns_error` (503-equivalent path)
  - `health_endpoint_ok`
  - `missing_tenant_header_is_unauthorized`
- `charts/feature-store/` — full Helm chart (Chart.yaml, values.yaml, _helpers.tpl,
  deployment.yaml, service.yaml, configmap.yaml, serviceaccount.yaml, networkpolicy.yaml)
- `gitops/dev/apps/feature-store.yaml` — ArgoCD child Application
- `gitops/dev/values/feature-store.yaml` — dev env overrides

### Bug fixed
`monte-carlo-sim/src/lib.rs` — replaced the single-step LCG with a splitmix64 finalizer.
The original LCG produced a non-uniform distribution for consecutive seeds 0..N, causing
the convergence proptest to fail at p=0.1 (empirical win rate ≈ 20%, expected ≤ 5% error).
splitmix64 is a high-quality bijective hash; all 3 proptests now pass with 256 cases each.

---

## Build + test results

```
cargo build --workspace         ✅  Finished dev profile — 0 errors
cargo test --workspace          ✅  All test result lines: ok — 0 failures
                                    (includes proptests for decision-arith, cost-engine,
                                     feature-store-types, monte-carlo-sim x3)
cargo test --test e2e_smoke \
  -p api-gateway --include-ignored  ✅  4/4 passed:
                                      health_endpoint_ok
                                      missing_tenant_header_is_unauthorized
                                      feature_store_grpc_unavailable_returns_error
                                      graphql_feature_rls_smoke (RLS isolation confirmed)
```

## Helm lint + YAML validation

```
helm lint charts/feature-store/    ✅  1 chart linted, 0 failed (INFO icon only)
python yaml.safe_load on:
  gitops/dev/apps/feature-store.yaml   ✅
  gitops/dev/values/feature-store.yaml ✅
```

---

## Architecture note

The e2e test spawns feature-store-server **in-process** (via `tokio::spawn`) rather than
launching an external binary with `std::process::Command`. This keeps tests hermetic and
fast — no path issues, no race on port binding. The `spawn_feature_store` helper binds to
a random free port so parallel test runs cannot collide.

The api-gateway connects to feature-store via `Channel::connect_lazy()` — the channel is
created at startup but the connection is not established until the first RPC. This means
the gateway starts even if feature-store is temporarily down; the GraphQL resolver returns
a structured error on the first failed call (tested by `feature_store_grpc_unavailable_returns_error`).

---

## What Sprint 3 needs to follow up on

### 1. mTLS between api-gateway and feature-store
Currently the gRPC channel is plaintext (`http://`). In production both services should
present certificates to each other. Tonic supports this via `ClientTlsConfig` /
`ServerTlsConfig`. Estimated: S-sized story.

### 2. OpenTelemetry spans across the boundary
Each gRPC call from api-gateway to feature-store should propagate a trace context header
so spans are correlated in Jaeger / Grafana Tempo. Add `tonic-tracing` / `tower-tracing`
middleware to both services. Estimated: M-sized.

### 3. `grpc.health.v1` service on feature-store
Replace the TCP-connect liveness/readiness probes in the Helm chart with a real gRPC
health check. Tonic ships `tonic-health` for this. Estimated: S-sized.

### 4. Pin dependency versions (Sprint 2 task from S1.10 handoff)
Run `cargo update` and commit `Cargo.lock` with pinned versions. The workspace currently
uses range versions only.

### 5. Dockerfile for feature-store binary
Analogous to the api-gateway Dockerfile. Needed before ArgoCD Image Updater can push new
builds automatically. Estimated: S-sized.

### 6. `repoURL` in gitops/ manifests
Replace the placeholder `https://github.com/openclaw/judicialpredict.git` with the real
private repo URL before bootstrapping ArgoCD on a real cluster.
