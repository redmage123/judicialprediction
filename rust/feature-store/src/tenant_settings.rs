// Tenant-scoped feature-tier override store (S2.12, JP-35).
//
// ADR-FP-001 split:
//   Functional core — TenantOverrides, FeatureTier, check_feature_allowed,
//                     diff_overrides: pure types and functions; no I/O.
//   Imperative shell — OverridesCache (Arc<DashMap>), get_overrides,
//                      update_overrides: async DB access + 60-second cache.
//
// The gRPC handlers in server.rs call get_overrides() then pass the result
// to check_feature_allowed(); the pure function is independently unit-testable.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use audit_recorder::{hash_payload, AuditEvent, AuditRecorder, AuditStatus};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// 60-second in-process TTL for loaded overrides.
const CACHE_TTL: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// Domain types — pure, no I/O
// ---------------------------------------------------------------------------

/// Feature tier values valid as override targets.
///
/// Only **tightening** is permitted: a tenant may downgrade a feature to
/// effectively Tier-C (refuse it entirely) but cannot grant a feature that the
/// global tier policy forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureTier {
    #[serde(rename = "TIER_A")]
    TierA,
    #[serde(rename = "TIER_B")]
    TierB,
    /// Refusing tier: the feature-store returns PERMISSION_DENIED for features
    /// downgraded to Tier-C via an override.
    #[serde(rename = "TIER_C")]
    TierC,
}

/// Per-tenant override configuration deserialized from the
/// `tenant_settings.feature_tier_overrides` jsonb column.
///
/// JSON shape:
/// ```json
/// {
///   "disabled_features": ["attorney_personality_score"],
///   "tier_overrides":    {"attorney_temperament": "TIER_C"}
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TenantOverrides {
    /// Feature names in this set are hard-refused regardless of tier.
    #[serde(default)]
    pub disabled_features: HashSet<String>,
    /// Per-feature tier overrides.  Tier-C entries cause PERMISSION_DENIED.
    #[serde(default)]
    pub tier_overrides: HashMap<String, FeatureTier>,
}

// ---------------------------------------------------------------------------
// Pure enforcement function — testable without I/O
// ---------------------------------------------------------------------------

/// Returns `Some(reason_message)` if the named feature is blocked by the
/// tenant's override policy, `None` if it is permitted.
///
/// The gRPC handlers map `Some(reason)` → `Status::permission_denied(reason)`.
pub fn check_feature_allowed(overrides: &TenantOverrides, feature_name: &str) -> Option<String> {
    if overrides.disabled_features.contains(feature_name) {
        return Some(format!(
            "feature '{feature_name}' is disabled for this tenant by override policy"
        ));
    }
    if let Some(FeatureTier::TierC) = overrides.tier_overrides.get(feature_name) {
        return Some(format!(
            "feature '{feature_name}' has been downgraded to Tier-C by tenant override; \
             supply an explicit PermittedUse to access protected-class features"
        ));
    }
    None
}

// ---------------------------------------------------------------------------
// Diff helper — pure, used by update_overrides
// ---------------------------------------------------------------------------

/// Collect all feature names that changed (added, removed, or value-changed)
/// between `old` and `new` overrides.  Returns an empty Vec if identical.
pub fn diff_overrides(old: &TenantOverrides, new: &TenantOverrides) -> Vec<String> {
    let mut changed: HashSet<String> = HashSet::new();

    // Names added to or removed from disabled_features.
    for f in old.disabled_features.symmetric_difference(&new.disabled_features) {
        changed.insert(f.clone());
    }

    // tier_overrides: keys that were added, removed, or changed value.
    let all_keys: HashSet<&str> = old
        .tier_overrides
        .keys()
        .chain(new.tier_overrides.keys())
        .map(String::as_str)
        .collect();
    for k in all_keys {
        if old.tier_overrides.get(k) != new.tier_overrides.get(k) {
            changed.insert(k.to_string());
        }
    }

    changed.into_iter().collect()
}

// ---------------------------------------------------------------------------
// In-process 60-second override cache
// ---------------------------------------------------------------------------

struct CacheEntry {
    overrides: TenantOverrides,
    loaded_at: Instant,
}

/// Thread-safe 60-second TTL cache of `TenantOverrides` keyed by `tenant_id`.
///
/// Wraps `Arc<DashMap>` so clone is cheap (Arc refcount bump only).
/// Invalidated by `update_overrides` immediately after a successful UPSERT so
/// the next read fetches fresh data from Postgres.
#[derive(Clone)]
pub struct OverridesCache(Arc<DashMap<Uuid, CacheEntry>>);

impl OverridesCache {
    pub fn new() -> Self {
        Self(Arc::new(DashMap::new()))
    }

    fn get_cached(&self, tenant_id: Uuid) -> Option<TenantOverrides> {
        let entry = self.0.get(&tenant_id)?;
        if entry.loaded_at.elapsed() < CACHE_TTL {
            Some(entry.overrides.clone())
        } else {
            // Stale; caller will re-fetch.
            None
        }
    }

    fn set_cached(&self, tenant_id: Uuid, overrides: TenantOverrides) {
        self.0.insert(
            tenant_id,
            CacheEntry {
                overrides,
                loaded_at: Instant::now(),
            },
        );
    }

    /// Evict the cache entry for a tenant (called after update_overrides).
    pub fn invalidate(&self, tenant_id: Uuid) {
        self.0.remove(&tenant_id);
    }
}

impl Default for OverridesCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// DB access — imperative shell
// ---------------------------------------------------------------------------

/// Load `TenantOverrides` for a tenant, checking the 60-second cache first.
///
/// Returns `TenantOverrides::default()` (no overrides applied) if no row
/// exists in `tenant_settings` for this tenant yet — this is the normal state
/// for tenants that have not configured any overrides.
pub async fn get_overrides(
    pool: &PgPool,
    tenant_id: Uuid,
    cache: &OverridesCache,
) -> Result<TenantOverrides> {
    if let Some(cached) = cache.get_cached(tenant_id) {
        return Ok(cached);
    }

    // Open a transaction and set the RLS context before querying.
    let set_sql = format!("SET LOCAL app.current_tenant_id = '{tenant_id}'");
    let mut tx = pool.begin().await.context("tenant_settings: begin tx")?;
    sqlx::query(&set_sql)
        .execute(&mut *tx)
        .await
        .context("tenant_settings: SET LOCAL")?;

    let row: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT feature_tier_overrides FROM tenant_settings WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .context("tenant_settings: SELECT")?;

    tx.commit().await.context("tenant_settings: commit SELECT")?;

    let overrides: TenantOverrides = match row {
        None => TenantOverrides::default(),
        Some(v) => serde_json::from_value(v).context("tenant_settings: deserialize jsonb")?,
    };

    cache.set_cached(tenant_id, overrides.clone());
    Ok(overrides)
}

/// Persist new overrides for a tenant, diff + audit-log every changed key.
///
/// Steps:
/// 1. Load current overrides (for diff computation).
/// 2. UPSERT new overrides into `tenant_settings`.
/// 3. Invalidate the cache entry.
/// 4. Write one `audit-recorder` event per added/removed/changed override key,
///    with `action = "tenant_settings.override_change"`.
pub async fn update_overrides(
    pool: &PgPool,
    tenant_id: Uuid,
    new_overrides: TenantOverrides,
    cache: &OverridesCache,
    recorder: &AuditRecorder,
) -> Result<()> {
    // Load current state for diff (cache hit is fine here).
    let old = get_overrides(pool, tenant_id, cache).await?;
    let changed_keys = diff_overrides(&old, &new_overrides);

    // Serialize and UPSERT.
    let new_json = serde_json::to_value(&new_overrides)
        .context("update_overrides: serialize new overrides")?;

    let set_sql = format!("SET LOCAL app.current_tenant_id = '{tenant_id}'");
    let mut tx = pool.begin().await.context("update_overrides: begin tx")?;
    sqlx::query(&set_sql)
        .execute(&mut *tx)
        .await
        .context("update_overrides: SET LOCAL")?;

    sqlx::query(
        r#"
        INSERT INTO tenant_settings (tenant_id, feature_tier_overrides)
        VALUES ($1, $2)
        ON CONFLICT (tenant_id) DO UPDATE SET
            feature_tier_overrides = EXCLUDED.feature_tier_overrides,
            updated_at             = now()
        "#,
    )
    .bind(tenant_id)
    .bind(&new_json)
    .execute(&mut *tx)
    .await
    .context("update_overrides: UPSERT")?;

    tx.commit().await.context("update_overrides: commit")?;

    // Invalidate before recording audit so re-reads reflect the new data.
    cache.invalidate(tenant_id);

    // One audit event per changed override key.
    let payload_hash = hash_payload(new_json.to_string().as_bytes());
    for key in &changed_keys {
        let event = AuditEvent {
            actor: "feature-store-admin".to_string(),
            action: "tenant_settings.override_change".to_string(),
            payload_hash: payload_hash.clone(),
            latency_ms: 0,
            status: AuditStatus::Ok,
            cost_micros: None,
        };
        recorder
            .record(tenant_id, event)
            .await
            .with_context(|| format!("update_overrides: audit for key '{key}'"))?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests — pure logic only, no I/O
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── check_feature_allowed ─────────────────────────────────────────────

    #[test]
    fn empty_overrides_allows_all_features() {
        let overrides = TenantOverrides::default();
        assert!(check_feature_allowed(&overrides, "judge.reversal_rate").is_none());
        assert!(check_feature_allowed(&overrides, "attorney.win_rate").is_none());
        assert!(check_feature_allowed(&overrides, "").is_none());
    }

    #[test]
    fn disabled_feature_returns_permission_denied() {
        let mut overrides = TenantOverrides::default();
        overrides
            .disabled_features
            .insert("attorney_personality_score".to_string());

        // Blocked.
        let result = check_feature_allowed(&overrides, "attorney_personality_score");
        assert!(result.is_some(), "disabled feature must be blocked");
        assert!(result.unwrap().contains("disabled"));

        // Other features remain unaffected.
        assert!(check_feature_allowed(&overrides, "judge.reversal_rate").is_none());
    }

    #[test]
    fn tier_c_override_returns_permission_denied() {
        let mut overrides = TenantOverrides::default();
        overrides
            .tier_overrides
            .insert("attorney_temperament".to_string(), FeatureTier::TierC);

        let result = check_feature_allowed(&overrides, "attorney_temperament");
        assert!(result.is_some(), "Tier-C override must be blocked");
        assert!(result.unwrap().contains("Tier-C"));

        // Tier-A and Tier-B overrides on other keys are fine.
        overrides
            .tier_overrides
            .insert("judge.circuit_experience".to_string(), FeatureTier::TierA);
        assert!(check_feature_allowed(&overrides, "judge.circuit_experience").is_none());
    }

    #[test]
    fn overrides_json_roundtrip() {
        let mut overrides = TenantOverrides::default();
        overrides
            .disabled_features
            .insert("judge_age_years".to_string());
        overrides
            .tier_overrides
            .insert("attorney_personality_score".to_string(), FeatureTier::TierC);
        overrides
            .tier_overrides
            .insert("case_duration_days".to_string(), FeatureTier::TierB);

        let json = serde_json::to_string(&overrides).expect("serialize");
        let decoded: TenantOverrides = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, overrides, "overrides must survive JSON round-trip");
    }

    // ── diff_overrides ────────────────────────────────────────────────────

    #[test]
    fn diff_detects_added_and_removed_disabled_features() {
        let mut old = TenantOverrides::default();
        old.disabled_features.insert("feature_x".to_string());

        let mut new = TenantOverrides::default();
        new.disabled_features.insert("feature_y".to_string());
        // feature_x removed, feature_y added → 2 changes

        let changed = diff_overrides(&old, &new);
        assert_eq!(changed.len(), 2);
        let changed_set: HashSet<&str> = changed.iter().map(String::as_str).collect();
        assert!(changed_set.contains("feature_x"));
        assert!(changed_set.contains("feature_y"));
    }

    #[test]
    fn diff_detects_tier_override_changes() {
        let mut old = TenantOverrides::default();
        old.tier_overrides
            .insert("feat_a".to_string(), FeatureTier::TierB);

        let mut new = TenantOverrides::default();
        new.tier_overrides
            .insert("feat_a".to_string(), FeatureTier::TierC); // value changed
        new.tier_overrides
            .insert("feat_b".to_string(), FeatureTier::TierC); // newly added

        let changed = diff_overrides(&old, &new);
        assert_eq!(changed.len(), 2);
        let changed_set: HashSet<&str> = changed.iter().map(String::as_str).collect();
        assert!(changed_set.contains("feat_a"));
        assert!(changed_set.contains("feat_b"));
    }

    #[test]
    fn diff_returns_empty_for_identical_overrides() {
        let mut overrides = TenantOverrides::default();
        overrides.disabled_features.insert("feat_x".to_string());
        overrides
            .tier_overrides
            .insert("feat_y".to_string(), FeatureTier::TierC);

        let changed = diff_overrides(&overrides, &overrides.clone());
        assert!(changed.is_empty(), "identical overrides must produce no diff");
    }
}
