// Imperative shell: I/O via sqlx + Postgres.
// Pure logic delegates to feature-store-types.

pub use feature_store_types::{PermittedUse, Sensitivity, Tier, TieredFeature};

/// Placeholder repository — full implementation in Sprint 2 once
/// the schema migration (ADR-001 §US-M1-01) is applied.
pub struct FeatureStoreRepo;

impl FeatureStoreRepo {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FeatureStoreRepo {
    fn default() -> Self {
        Self::new()
    }
}
