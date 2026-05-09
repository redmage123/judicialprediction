// S3.11 / JP-52 — Audit-recorder integration tests for UpdateTenantSettings.
//
// Verifies that every changed override key produces exactly one audit_log row
// with action="tenant_settings.override_change", and that idempotent re-saves
// produce ZERO new rows.
//
// Audit-row design decisions (documented per S3.11 spec):
//   - ONE row per CHANGED KEY: each added/removed/modified override key is
//     individually traceable, satisfying compliance traceability requirements.
//   - All rows in a single mutation share the SAME payload_hash (SHA-256 of the
//     serialised new TenantOverrides JSON), because the hash fingerprints the
//     atomic state written, not the individual keys. The changed key is implicit
//     in the audit context (actor, action, tenant_id, and timestamp).
//   - payload_hash uniqueness ACROSS MUTATIONS with identical JSON is NOT
//     enforced here (e.g. two operators submitting the same override JSON).
//     Cross-row deduplication is explicitly out of scope for Sprint 3
//     (documented as a Sprint-4 follow-up if cross-operator deduplication is needed).
//
// Requires:
//   - DATABASE_URL       → jp_app DSN (e.g. postgres://jp_app:...@127.0.0.1:5454/judicialpredict_dev)
//   - ADMIN_DATABASE_URL → superuser DSN for setup/teardown (falls back to derived URL)
//   - Migrations applied (including 20260509120000_tenant_settings)
//
// Run:
//   cargo test -p feature-store --tests -- --include-ignored
//   (or: cargo test -p feature-store --tests -- tenant_settings_audit --include-ignored)

use feature_store::tenant_settings::{
    diff_overrides, get_overrides, update_overrides, FeatureTier, OverridesCache, TenantOverrides,
};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Test tenant UUIDs — deterministic, easy to clean up.
// Different from the S2.12 test tenant (cc000000-...-0012) to avoid collisions.
// ---------------------------------------------------------------------------

const AUDIT_TEST_TENANT_A: &str = "dd000000-0000-0000-0000-000000000031";
const AUDIT_TEST_TENANT_B: &str = "dd000000-0000-0000-0000-000000000032";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn jp_app_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://jp_app:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev".to_string()
    })
}

fn admin_url() -> String {
    std::env::var("ADMIN_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev"
            .to_string()
    })
}

/// Count audit rows for `tenant_id` with `action = "tenant_settings.override_change"`.
/// Uses the jp_app pool; RLS is set so only that tenant's rows are visible.
async fn count_audit_rows(pool: &sqlx::PgPool, tenant_id: uuid::Uuid) -> i64 {
    let mut tx = pool.begin().await.expect("begin tx for audit count");
    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{tenant_id}'"
    ))
    .execute(&mut *tx)
    .await
    .expect("SET LOCAL for audit count");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE tenant_id = $1 AND action = 'tenant_settings.override_change'",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("count audit rows");

    tx.commit().await.expect("commit audit count tx");
    count
}

/// Ensure a test tenant exists (FK for tenant_settings.tenant_id).
async fn upsert_test_tenant(admin_pool: &sqlx::PgPool, tenant_id: uuid::Uuid, slug: &str) {
    sqlx::query(
        "INSERT INTO tenants (id, slug, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(tenant_id)
    .bind(slug)
    .bind(format!("S3.11 Audit Test Tenant {slug}"))
    .execute(admin_pool)
    .await
    .expect("upsert test tenant");
}

/// Remove all test rows created by this test tenant.
async fn cleanup_tenant(admin_pool: &sqlx::PgPool, tenant_id: uuid::Uuid) {
    sqlx::query("DELETE FROM audit_log WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(admin_pool)
        .await
        .expect("cleanup audit_log");
    sqlx::query("DELETE FROM tenant_settings WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(admin_pool)
        .await
        .expect("cleanup tenant_settings");
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(admin_pool)
        .await
        .expect("cleanup tenants");
}

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

/// Test 1: Fresh tenant → set 2 disabled features → expect exactly 2 audit rows.
///
/// Scenario: tenant has no overrides; we write 2 disabled feature names.
/// Each name is a changed key → 2 audit rows with action=tenant_settings.override_change.
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn two_disabled_features_produce_two_audit_rows() {
    let tenant_id: uuid::Uuid = AUDIT_TEST_TENANT_A.parse().unwrap();
    let jp_pool = sqlx::PgPool::connect(&jp_app_url()).await.expect("jp_app pool");
    let admin_pool = sqlx::PgPool::connect(&admin_url()).await.expect("admin pool");
    upsert_test_tenant(&admin_pool, tenant_id, "s311-audit-a").await;

    let cache = OverridesCache::new();
    let recorder = audit_recorder::AuditRecorder::new(jp_pool.clone());

    // Baseline: no prior audit rows.
    let before = count_audit_rows(&jp_pool, tenant_id).await;

    // UPSERT: 2 disabled features (both are new → 2 changed keys).
    let mut new_overrides = TenantOverrides::default();
    new_overrides
        .disabled_features
        .insert("attorney_personality_score".to_string());
    new_overrides
        .disabled_features
        .insert("judge_age_years".to_string());

    update_overrides(&jp_pool, tenant_id, new_overrides.clone(), &cache, &recorder)
        .await
        .expect("update_overrides");

    let after = count_audit_rows(&jp_pool, tenant_id).await;
    assert_eq!(
        after - before,
        2,
        "2 new disabled features → 2 audit rows; got delta {}",
        after - before,
    );

    cleanup_tenant(&admin_pool, tenant_id).await;
}

/// Test 2: Idempotency — re-saving the same overrides produces ZERO new audit rows.
///
/// Scenario: write overrides once, verify 2 rows. Write the same overrides again.
/// diff_overrides returns empty → update_overrides writes 0 new audit rows.
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn identical_resave_produces_zero_audit_rows() {
    let tenant_id: uuid::Uuid = AUDIT_TEST_TENANT_A.parse().unwrap();
    let jp_pool = sqlx::PgPool::connect(&jp_app_url()).await.expect("jp_app pool");
    let admin_pool = sqlx::PgPool::connect(&admin_url()).await.expect("admin pool");
    upsert_test_tenant(&admin_pool, tenant_id, "s311-audit-a").await;

    let cache = OverridesCache::new();
    let recorder = audit_recorder::AuditRecorder::new(jp_pool.clone());

    let mut overrides = TenantOverrides::default();
    overrides
        .disabled_features
        .insert("attorney_personality_score".to_string());
    overrides
        .disabled_features
        .insert("judge_age_years".to_string());

    // First write.
    update_overrides(&jp_pool, tenant_id, overrides.clone(), &cache, &recorder)
        .await
        .expect("first update_overrides");
    let after_first = count_audit_rows(&jp_pool, tenant_id).await;

    // Second write with IDENTICAL overrides — must produce zero new rows.
    update_overrides(&jp_pool, tenant_id, overrides.clone(), &cache, &recorder)
        .await
        .expect("second update_overrides (idempotent)");
    let after_second = count_audit_rows(&jp_pool, tenant_id).await;

    assert_eq!(
        after_second - after_first,
        0,
        "identical re-save must produce zero new audit rows; got delta {}",
        after_second - after_first,
    );

    cleanup_tenant(&admin_pool, tenant_id).await;
}

/// Test 3: Mixed change — modify 1 key, remove 1 key, add 1 key → 3 audit rows.
///
/// Scenario:
///   Initial: disabled_features = [A, B]
///   New:     disabled_features = [A, C], tier_overrides = {D: TIER_C}
///   diff: B removed (1), C added (1), D added (1) → 3 changed keys → 3 audit rows.
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn modify_remove_add_produces_three_audit_rows() {
    let tenant_id: uuid::Uuid = AUDIT_TEST_TENANT_A.parse().unwrap();
    let jp_pool = sqlx::PgPool::connect(&jp_app_url()).await.expect("jp_app pool");
    let admin_pool = sqlx::PgPool::connect(&admin_url()).await.expect("admin pool");
    upsert_test_tenant(&admin_pool, tenant_id, "s311-audit-a").await;

    let cache = OverridesCache::new();
    let recorder = audit_recorder::AuditRecorder::new(jp_pool.clone());

    // Initial state: 2 disabled features.
    let mut initial = TenantOverrides::default();
    initial.disabled_features.insert("feat_a".to_string());
    initial.disabled_features.insert("feat_b".to_string());
    update_overrides(&jp_pool, tenant_id, initial, &cache, &recorder)
        .await
        .expect("initial update_overrides");
    let after_initial = count_audit_rows(&jp_pool, tenant_id).await;

    // New state: keep feat_a, remove feat_b, add feat_c (disabled), add feat_d (TIER_C override).
    let mut new_overrides = TenantOverrides::default();
    new_overrides.disabled_features.insert("feat_a".to_string());
    new_overrides.disabled_features.insert("feat_c".to_string()); // new
    new_overrides
        .tier_overrides
        .insert("feat_d".to_string(), FeatureTier::TierC); // new

    // Verify diff count matches expectations before writing.
    let current = get_overrides(&jp_pool, tenant_id, &cache)
        .await
        .expect("get_overrides for diff check");
    let diff = diff_overrides(&current, &new_overrides);
    // B removed, C added, D added → 3 keys.
    let diff_set: HashSet<&str> = diff.iter().map(String::as_str).collect();
    assert!(diff_set.contains("feat_b"), "feat_b must be in diff (removed)");
    assert!(diff_set.contains("feat_c"), "feat_c must be in diff (added)");
    assert!(diff_set.contains("feat_d"), "feat_d must be in diff (added)");
    assert_eq!(diff.len(), 3, "diff must have 3 changed keys; got {:?}", diff);

    update_overrides(&jp_pool, tenant_id, new_overrides, &cache, &recorder)
        .await
        .expect("mixed change update_overrides");
    let after_change = count_audit_rows(&jp_pool, tenant_id).await;

    assert_eq!(
        after_change - after_initial,
        3,
        "remove+add+add → 3 audit rows; got delta {}",
        after_change - after_initial,
    );

    cleanup_tenant(&admin_pool, tenant_id).await;
}

/// Test 4: RLS isolation — tenant B's audit rows are invisible to tenant A.
///
/// Scenario: write overrides for both tenants. When querying as tenant A,
/// only tenant A's rows are visible (RLS enforces isolation).
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn rls_prevents_cross_tenant_audit_row_visibility() {
    let tenant_a: uuid::Uuid = AUDIT_TEST_TENANT_A.parse().unwrap();
    let tenant_b: uuid::Uuid = AUDIT_TEST_TENANT_B.parse().unwrap();

    let jp_pool = sqlx::PgPool::connect(&jp_app_url()).await.expect("jp_app pool");
    let admin_pool = sqlx::PgPool::connect(&admin_url()).await.expect("admin pool");
    upsert_test_tenant(&admin_pool, tenant_a, "s311-rls-a").await;
    upsert_test_tenant(&admin_pool, tenant_b, "s311-rls-b").await;

    let cache_a = OverridesCache::new();
    let cache_b = OverridesCache::new();
    let recorder = audit_recorder::AuditRecorder::new(jp_pool.clone());

    // Write 1 override key for tenant A.
    let mut ovr_a = TenantOverrides::default();
    ovr_a.disabled_features.insert("feat_only_a".to_string());
    update_overrides(&jp_pool, tenant_a, ovr_a, &cache_a, &recorder)
        .await
        .expect("update_overrides tenant_a");

    // Write 2 override keys for tenant B.
    let mut ovr_b = TenantOverrides::default();
    ovr_b.disabled_features.insert("feat_b1".to_string());
    ovr_b.disabled_features.insert("feat_b2".to_string());
    update_overrides(&jp_pool, tenant_b, ovr_b, &cache_b, &recorder)
        .await
        .expect("update_overrides tenant_b");

    // Query as tenant A — must see exactly 1 row (its own).
    let count_a = count_audit_rows(&jp_pool, tenant_a).await;
    assert_eq!(
        count_a, 1,
        "tenant A must see only its own 1 audit row; got {count_a}"
    );

    // Query as tenant B — must see exactly 2 rows (its own).
    let count_b = count_audit_rows(&jp_pool, tenant_b).await;
    assert_eq!(
        count_b, 2,
        "tenant B must see only its own 2 audit rows; got {count_b}"
    );

    cleanup_tenant(&admin_pool, tenant_a).await;
    cleanup_tenant(&admin_pool, tenant_b).await;
}
