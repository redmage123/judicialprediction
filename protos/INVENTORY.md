# Proto Package Inventory

Running list of every proto package, its version, owning plane, and status.
Updated whenever a package is added, versioned, deprecated, or sunset.

| Package | File | Plane | Version | Status | Added | Notes |
|---------|------|-------|---------|--------|-------|-------|
| `judicialpredict.data_plane.feature_store.v1` | `judicialpredict/data_plane/feature_store/v1/feature_store.proto` | Rust data plane | v1 | Active | 2026-05-07 | First contract. `FeatureStoreService`: GetFeature, ListFeatures (streaming), IngestFeature. Encodes Tier + Sensitivity ADTs per ADR-004. |
| `judicialpredict.ml_plane.inference.v1` | `judicialpredict/ml_plane/inference/v1/inference.proto` | Python ML plane | v1 | Active | 2026-05-07 | `InferenceService`: PredictCaseOutcome → p_win + ConformalInterval + ShapValues. Called by Rust api-gateway. |

## Status definitions

| Status | Meaning |
|--------|---------|
| Active | In use; breaking changes blocked by `buf breaking` CI gate |
| Deprecated | Superseded by a newer version; kept for backward compat; will be removed after migration |
| Sunset | Removed from active use; stubs must not be generated; kept in history for audit |

## Versioning policy (per ADR-002)

- Additive changes (new fields, new RPCs, new enum values) are **non-breaking** and may land in the same version.
- Renaming, removing, or changing field types/numbers is **breaking** and requires a new package version (e.g. `v2`).
- `buf breaking --against .git#branch=main` runs on every PR touching `protos/`.
- Package versions are not bumped without an ADR update recording the migration plan.
