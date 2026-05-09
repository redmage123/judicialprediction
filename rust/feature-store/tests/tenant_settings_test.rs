// Tests for the tenant-settings override store (S2.12, JP-35).
//
// Unit/property tests — pure logic, no DB:
//   1. empty_overrides_never_blocks_any_feature        (proptest)
//   2. disabled_feature_always_blocked                  (proptest)
//   3. tier_c_override_always_blocked                   (proptest)
//   4. tier_a_and_b_overrides_are_permitted             (unit)
//
// Integration test — requires Postgres (#[ignore]):
//   5. overrides_upsert_enforce_and_audit               (integration)

use feature_store::tenant_settings::{
    check_feature_allowed, update_overrides, FeatureTier, OverridesCache, TenantOverrides,
};
use proptest::prelude::*;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Proptest helpers
// ---------------------------------------------------------------------------

/// Strategy for valid feature names (ASCII identifier-like, 1–40 chars).
fn feature_name_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,38}[a-z0-9]"
}

// ---------------------------------------------------------------------------
// Property tests — pure logic (no I/O)
// ---------------------------------------------------------------------------

proptest! {
    /// An empty override set never blocks any feature, regardless of name.
    #[test]
    fn empty_overrides_never_blocks_any_feature(name in feature_name_strategy()) {
        let overrides = TenantOverrides::default();
        prop_assert!(
            check_feature_allowed(&overrides, &name).is_none(),
            "empty overrides must permit all features"
        );
    }

    /// For any tenant with disabled_features = [X], check_feature_allowed(X)
    /// must return Some(_) (PERMISSION_DENIED).
    #[test]
    fn disabled_feature_always_blocked(name in feature_name_strategy()) {
        let mut overrides = TenantOverrides::default();
        overrides.disabled_features.insert(name.clone());

        let result = check_feature_allowed(&overrides, &name);
        prop_assert!(
            result.is_some(),
            "feature in disabled_features must be blocked"
        );
        prop_assert!(
            result.unwrap().contains("disabled"),
            "denial reason must mention 'disabled'"
        );
    }

    /// For any feature with tier_overrides[X] = TIER_C, check_feature_allowed(X)
    /// must return Some(_).
    #[test]
    fn tier_c_override_always_blocked(name in feature_name_strategy()) {
        let mut overrides = TenantOverrides::default();
        overrides
            .tier_overrides
            .insert(name.clone(), FeatureTier::TierC);

        let result = check_feature_allowed(&overrides, &name);
        prop_assert!(
            result.is_some(),
            "Tier-C override must always be blocked"
        );
    }
}

// ---------------------------------------------------------------------------
// Unit tests — pure logic
// ---------------------------------------------------------------------------

/// Tier-A and Tier-B overrides do NOT block the feature — only Tier-C does.
#[test]
fn tier_a_and_b_overrides_are_permitted() {
    let mut overrides = TenantOverrides::default();
    overrides
        .tier_overrides
        .insert("judge.experience_years".to_string(), FeatureTier::TierA);
    overrides
        .tier_overrides
        .insert("case.prior_outcomes".to_string(), FeatureTier::TierB);

    assert!(
        check_feature_allowed(&overrides, "judge.experience_years").is_none(),
        "Tier-A override must not block the feature"
    );
    assert!(
        check_feature_allowed(&overrides, "case.prior_outcomes").is_none(),
        "Tier-B override must not block the feature"
    );
}

/// disabled_features and tier_overrides operate independently — both can
/// block a feature and independent sets don't interfere.
#[test]
fn disabled_and_tier_overrides_are_independent() {
    let mut overrides = TenantOverrides::default();
    overrides
        .disabled_features
        .insert("blocked_feature".to_string());
    overrides
        .tier_overrides
        .insert("downgraded_feature".to_string(), FeatureTier::TierC);

    // Each blocked individually.
    assert!(check_feature_allowed(&overrides, "blocked_feature").is_some());
    assert!(check_feature_allowed(&overrides, "downgraded_feature").is_some());

    // Unrelated feature is unaffected.
    assert!(check_feature_allowed(&overrides, "free_feature").is_none());
}

/// TenantOverrides serializes and deserializes cleanly — no field loss.
#[test]
fn tenant_overrides_json_roundtrip() {
    let mut overrides = TenantOverrides::default();
    overrides
        .disabled_features
        .insert("attorney_personality_score".to_string());
    overrides
        .disabled_features
        .insert("judge_age_years".to_string());
    overrides
        .tier_overrides
        .insert("attorney_temperament".to_string(), FeatureTier::TierC);
    overrides
        .tier_overrides
        .insert("case_complexity_index".to_string(), FeatureTier::TierB);

    let json = serde_json::to_string(&overrides).expect("serialize");
    let decoded: TenantOverrides =
        serde_json::from_str(&json).expect("deserialize");

    assert_eq!(decoded.disabled_features, overrides.disabled_features);
    assert_eq!(decoded.tier_overrides, overrides.tier_overrides);
}

/// Denial message for a disabled feature mentions the feature name.
#[test]
fn denial_message_contains_feature_name() {
    let feature = "attorney_personality_score";
    let mut overrides = TenantOverrides::default();
    overrides.disabled_features.insert(feature.to_string());

    let reason = check_feature_allowed(&overrides, feature).expect("must deny");
    assert!(
        reason.contains(feature),
        "denial message must contain the feature name; got: {reason}"
    );
}

// ---------------------------------------------------------------------------
// Integration test — requires live Postgres dev stack
// ---------------------------------------------------------------------------

/// Full round-trip: UPSERT overrides → get_overrides cache miss → enforcement →
/// audit row written → cleanup.
///
/// Requires:
///   - `DATABASE_URL`       env var pointing at the jp_app DSN.
///   - `ADMIN_DATABASE_URL` env var for superuser cleanup (or derived automatically).
///   - Migrations applied (including 20260509120000_tenant_settings).
///
/// Run:
///   cargo test -p feature-store --tests -- --include-ignored
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn overrides_upsert_enforce_and_audit() {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://jp_app:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev".to_string()
    });
    let admin_url = std::env::var("ADMIN_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev"
            .to_string()
    });

    let jp_pool = sqlx::PgPool::connect(&db_url)
        .await
        .expect("jp_app pool connect");
    let admin_pool = sqlx::PgPool::connect(&admin_url)
        .await
        .expect("admin pool connect");

    // Fixed tenant IDs for this test (deterministic, easy to clean up).
    let tenant_id: uuid::Uuid = "cc000000-0000-0000-0000-000000000012"
        .parse()
        .unwrap();

    // Ensure the test tenant exists (FK constraint on tenant_settings.tenant_id).
    sqlx::query(
        "INSERT INTO tenants (id, slug, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(tenant_id)
    .bind("s212-test-tenant")
    .bind("S2.12 Integration Test Tenant")
    .execute(&admin_pool)
    .await
    .expect("upsert test tenant");

    let cache = OverridesCache::new();
    let recorder = audit_recorder::AuditRecorder::new(jp_pool.clone());

    // ── 1. No overrides yet → default (everything permitted) ────────────────
    let initial = feature_store::tenant_settings::get_overrides(&jp_pool, tenant_id, &cache)
        .await
        .expect("get initial overrides");
    assert_eq!(
        initial,
        TenantOverrides::default(),
        "no row yet → default overrides"
    );
    assert!(
        check_feature_allowed(&initial, "attorney_personality_score").is_none(),
        "no override → feature must be permitted"
    );

    // ── 2. UPSERT overrides ──────────────────────────────────────────────────
    let mut new_overrides = TenantOverrides::default();
    new_overrides
        .disabled_features
        .insert("attorney_personality_score".to_string());
    new_overrides
        .tier_overrides
        .insert("judge_age_years".to_string(), FeatureTier::TierC);

    update_overrides(
        &jp_pool,
        tenant_id,
        new_overrides.clone(),
        &cache,
        &recorder,
    )
    .await
    .expect("update_overrides");

    // ── 3. Cache was invalidated; re-fetch must hit the DB ──────────────────
    let loaded = feature_store::tenant_settings::get_overrides(&jp_pool, tenant_id, &cache)
        .await
        .expect("get overrides after upsert");

    assert_eq!(
        loaded.disabled_features,
        HashSet::from(["attorney_personality_score".to_string()]),
        "disabled_features must be persisted and loaded"
    );
    assert_eq!(
        loaded.tier_overrides.get("judge_age_years"),
        Some(&FeatureTier::TierC),
        "tier_override must be persisted and loaded"
    );

    // ── 4. Enforcement: disabled feature is blocked ──────────────────────────
    let denied = check_feature_allowed(&loaded, "attorney_personality_score");
    assert!(denied.is_some(), "disabled feature must be blocked after upsert");

    // ── 5. Enforcement: tier_c override is blocked ────────────────────────────
    let denied_tier = check_feature_allowed(&loaded, "judge_age_years");
    assert!(denied_tier.is_some(), "Tier-C override must be blocked after upsert");

    // ── 6. Unrelated feature is still permitted ──────────────────────────────
    assert!(
        check_feature_allowed(&loaded, "judge.reversal_rate").is_none(),
        "unrelated feature must remain permitted"
    );

    // ── 7. Audit row was written (2 changed keys → 2 audit rows) ────────────
    let count: i64 = {
        let mut tx = jp_pool.begin().await.expect("begin tx");
        sqlx::query(&format!(
            "SET LOCAL app.current_tenant_id = '{tenant_id}'"
        ))
        .execute(&mut *tx)
        .await
        .expect("SET LOCAL");

        sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log
             WHERE tenant_id = $1 AND action = 'tenant_settings.override_change'",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .expect("count audit rows")
    };

    assert!(
        count >= 2,
        "must have at least 2 audit rows (one per changed key); got {count}"
    );

    // ── 8. Cleanup ───────────────────────────────────────────────────────────
    sqlx::query("DELETE FROM audit_log WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&admin_pool)
        .await
        .expect("cleanup audit_log");
    sqlx::query("DELETE FROM tenant_settings WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&admin_pool)
        .await
        .expect("cleanup tenant_settings");
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&admin_pool)
        .await
        .expect("cleanup tenants");
}
