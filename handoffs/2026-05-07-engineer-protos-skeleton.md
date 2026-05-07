# Handoff — protos/ skeleton + first contracts

**From:** gigforge-engineer (Chris Novak)
**To:** PM / next engineer
**Date:** 2026-05-07
**Plane issue:** JP-3

---

## Status: COMPLETE — buf lint passes (exit 0)

```
$ buf lint
(no output — all rules pass)
EXIT:0
```

buf version: 1.69.0 (installed at `/usr/local/bin/buf`).

---

## Files Created

```
protos/
├── buf.yaml                                          buf module config, STANDARD ruleset
├── buf.gen.yaml                                      codegen: prost+tonic (Rust), grpcio (Python)
├── INVENTORY.md                                      running package/version/status registry
└── judicialpredict/
    ├── data_plane/
    │   └── feature_store/
    │       └── v1/
    │           └── feature_store.proto               FeatureStoreService (data plane)
    └── ml_plane/
        └── inference/
            └── v1/
                inference.proto                        InferenceService (ML plane)
```

---

## What each file contains

### feature_store.proto
- **Package:** `judicialpredict.data_plane.feature_store.v1`
- **Enums:** `Tier` (UNSPECIFIED/A/B/C/D), `Sensitivity` (UNSPECIFIED/PUBLIC/QUASI_PUBLIC/INFERRED/PROTECTED), `PermittedUse` (UNSPECIFIED/DISPARATE_IMPACT_AUDIT/OPERATOR_OVERRIDE_WITH_AUDIT_LOG)
- **Messages:** `Feature`, `GetFeatureRequest`, `GetFeatureResponse`, `ListFeaturesRequest`, `ListFeaturesResponse`, `IngestFeatureRequest`, `IngestFeatureResponse`
- **Service:** `FeatureStoreService` — `GetFeature`, `ListFeatures` (server-streaming), `IngestFeature`
- Tier/Sensitivity enums mirror the Rust ADTs in `feature-store-types` (ADR-004)

### inference.proto
- **Package:** `judicialpredict.ml_plane.inference.v1`
- **Enums:** `ModelVariant` (UNSPECIFIED/XGBOOST/BAYESIAN_JUDGE/RF_ATTORNEY/META_LEARNER)
- **Messages:** `ShapValue`, `ConformalInterval`, `PredictCaseOutcomeRequest`, `PredictCaseOutcomeResponse`
- **Service:** `InferenceService` — `PredictCaseOutcome` → p_win + ConformalInterval + repeated ShapValue
- Response includes `mlflow_run_id` for reproducibility / legal audit

---

## What the next person needs to do (Sprint 2)

### 1. Wire codegen into CI (DevOps story)
`buf.gen.yaml` is configured but codegen requires remote BSR plugins — needs a BSR token or local plugin install:
```bash
buf generate --template buf.gen.yaml
```
Output paths: `../rust/generated/proto/` and `../python/generated/proto/`.
Consider switching to local plugins (`protoc-gen-prost`, `protoc-gen-tonic`, `grpc_tools_node_protoc`) if BSR auth is not set up.

### 2. Add `buf breaking` CI gate
```yaml
# In CI on every PR touching protos/:
buf breaking --against '.git#branch=main'
```
This blocks field renames, field number changes, and RPC signature changes.

### 3. Integrate generated Rust stubs into `feature-store` and `api-gateway` crates
- Add `prost`, `tonic`, `tonic-build` to `[workspace.dependencies]`
- Add a `build.rs` to `feature-store` that calls `tonic_build::compile_protos`
- Or use the BSR-generated crate once `buf generate` is wired

### 4. Integrate generated Python stubs into `ml-inference-svc`
```bash
python -m grpc_tools.protoc \
  -I protos \
  --python_out=python/ml-inference-svc/generated \
  --grpc_python_out=python/ml-inference-svc/generated \
  protos/judicialpredict/ml_plane/inference/v1/inference.proto
```

### 5. Add `feature-store-types` ↔ proto round-trip tests
Property tests asserting that every `Tier` Rust variant maps bijectively to the `Tier` proto enum value. Prevents the two representations drifting.

---

## Notes

- Directory layout follows buf PACKAGE_DIRECTORY_MATCH rule: package path must match directory path relative to `buf.yaml`. Do not move `.proto` files without updating their `package` declaration.
- `TIER_D` is present in the enum (reserved for future regulatory expansion). It does not have a corresponding Rust ADT variant yet — add both together when the need arises.
- `buf lint` used `STANDARD` ruleset with `ENUM_VALUE_PREFIX` excepted — all `Sensitivity` values carry the `SENSITIVITY_` prefix to satisfy the rule anyway; the exception was written defensively.
