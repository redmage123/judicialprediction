// Cross-plane RLS integration test for the audit-recorder crate.
//
// Requirements:
//   - Docker Compose dev stack running with Postgres on port 5454.
//   - All migrations applied (including 20260507120004 which enables RLS on audit_log).
//   - Env vars (optional, fall back to dev-stack defaults):
//       DATABASE_URL       — jp_app DSN  (non-superuser, RLS is enforced)
//       ADMIN_DATABASE_URL — superuser DSN (BYPASSRLS, used for cleanup)
//
// Run explicitly:
//   cargo test -p audit-recorder --test rls_integration -- --include-ignored
//
// What these tests prove:
//   1. AuditRecorder::record() writes rows visible to the owning tenant.
//   2. RLS blocks cross-tenant SELECTs — tenant B cannot read tenant A's rows.
//   3. actor + action strings round-trip cleanly through INSERT → SELECT.

use audit_recorder::{AuditEvent, AuditRecorder, AuditStatus, hash_payload};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// DSN helpers
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

// ---------------------------------------------------------------------------
// Test tenant UUIDs
// ---------------------------------------------------------------------------

/// Tenant used as the "owner" of audit rows in these tests.
const TENANT_A: &str = "aa000000-0000-0000-0000-000000000001";
/// Tenant used to probe RLS isolation — it writes nothing in this test.
const TENANT_B: &str = "bb000000-0000-0000-0000-000000000002";

// ---------------------------------------------------------------------------
// S2.11 RLS isolation — primary cross-plane invariant test
// ---------------------------------------------------------------------------

/// Verify that audit rows for tenant A are invisible to tenant B contexts.
///
/// Steps:
///   1. Connect as jp_app (non-superuser, RLS enforced).
///   2. Record two audit events for tenant A and one for tenant B.
///   3. Set `app.current_tenant_id` to tenant A and count visible rows → expect 2.
///   4. Set `app.current_tenant_id` to tenant B and count visible rows → expect 1.
///   5. Confirm actor + action round-trip cleanly for the tenant A rows.
///   6. Clean up (admin pool bypasses RLS).
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn rls_tenant_isolation_and_actor_action_roundtrip() {
    // -----------------------------------------------------------------------
    // 1. Pools
    // -----------------------------------------------------------------------
    let jp_pool = PgPool::connect(&jp_app_url())
        .await
        .expect("jp_app pool connect");
    let admin_pool = PgPool::connect(&admin_url())
        .await
        .expect("admin pool connect");

    let recorder = AuditRecorder::new(jp_pool.clone());

    let tenant_a: Uuid = TENANT_A.parse().unwrap();
    let tenant_b: Uuid = TENANT_B.parse().unwrap();

    // -----------------------------------------------------------------------
    // 2. Ensure test tenants exist (required by FK on audit_log.tenant_id).
    // -----------------------------------------------------------------------
    for (id, name) in [(tenant_a, "rls-test-tenant-a"), (tenant_b, "rls-test-tenant-b")] {
        sqlx::query(
            "INSERT INTO tenants (id, name, plan) VALUES ($1, $2, 'free') ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(name)
        .execute(&admin_pool)
        .await
        .expect("upsert tenant");
    }

    // -----------------------------------------------------------------------
    // 3. Record two events for tenant A, one for tenant B.
    // -----------------------------------------------------------------------
    let event_a1 = AuditEvent {
        actor: "api-gateway".to_string(),
        action: "feature_store.GetFeature".to_string(),
        payload_hash: hash_payload(b"rls-test-payload-a1"),
        latency_ms: 12,
        status: AuditStatus::Ok,
        cost_micros: Some(500),
    };
    let event_a2 = AuditEvent {
        actor: "ml-inference-svc".to_string(),
        action: "predict.CaseOutcome".to_string(),
        payload_hash: hash_payload(b"rls-test-payload-a2"),
        latency_ms: 88,
        status: AuditStatus::Ok,
        cost_micros: None,
    };
    let event_b1 = AuditEvent {
        actor: "api-gateway".to_string(),
        action: "feature_store.GetFeature".to_string(),
        payload_hash: hash_payload(b"rls-test-payload-b1"),
        latency_ms: 25,
        status: AuditStatus::RateLimit,
        cost_micros: None,
    };

    recorder.record(tenant_a, event_a1.clone()).await.expect("record A1");
    recorder.record(tenant_a, event_a2.clone()).await.expect("record A2");
    recorder.record(tenant_b, event_b1).await.expect("record B1");

    // -----------------------------------------------------------------------
    // 4. Count rows visible to tenant A — must be exactly 2.
    // -----------------------------------------------------------------------
    let count_a: i64 = {
        let mut tx = jp_pool.begin().await.expect("begin tx");
        sqlx::query(&format!(
            "SET LOCAL app.current_tenant_id = '{tenant_a}'"
        ))
        .execute(&mut *tx)
        .await
        .expect("SET LOCAL tenant A");

        sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE subject_id = $1 OR subject_id = $2",
        )
        .bind(&event_a1.actor)
        .bind(&event_a2.actor)
        .fetch_one(&mut *tx)
        .await
        .expect("count for tenant A")
    };

    // -----------------------------------------------------------------------
    // 5. Count rows visible to tenant B — must be exactly 1 (its own row only).
    // -----------------------------------------------------------------------
    let count_b: i64 = {
        let mut tx = jp_pool.begin().await.expect("begin tx for B");
        sqlx::query(&format!(
            "SET LOCAL app.current_tenant_id = '{tenant_b}'"
        ))
        .execute(&mut *tx)
        .await
        .expect("SET LOCAL tenant B");

        // Querying for the same actors as A — but RLS should block them.
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE table_name = 'outbound_call'",
        )
        .fetch_one(&mut *tx)
        .await
        .expect("count for tenant B")
    };

    // -----------------------------------------------------------------------
    // 6. Verify actor + action round-trip — read back tenant A's rows.
    // -----------------------------------------------------------------------
    #[derive(sqlx::FromRow)]
    struct AuditRow {
        subject_id: String,
        action: String,
        reason_code: String,
        latency_ms: Option<i32>,
        cost_micros: Option<i32>,
    }

    let rows: Vec<AuditRow> = {
        let mut tx = jp_pool.begin().await.expect("begin tx for round-trip");
        sqlx::query(&format!(
            "SET LOCAL app.current_tenant_id = '{tenant_a}'"
        ))
        .execute(&mut *tx)
        .await
        .expect("SET LOCAL tenant A round-trip");

        sqlx::query_as(
            "SELECT subject_id, action, reason_code, latency_ms, cost_micros
             FROM audit_log
             WHERE tenant_id = $1
             ORDER BY id",
        )
        .bind(tenant_a)
        .fetch_all(&mut *tx)
        .await
        .expect("fetch rows for round-trip check")
    };

    // -----------------------------------------------------------------------
    // 7. Assertions
    // -----------------------------------------------------------------------
    assert_eq!(
        count_a, 2,
        "tenant A context must see exactly 2 audit rows; got {count_a}"
    );
    assert_eq!(
        count_b, 1,
        "tenant B context must see exactly 1 audit row (its own); RLS must block tenant A's rows; got {count_b}"
    );
    assert_eq!(rows.len(), 2, "round-trip fetch must return 2 rows for tenant A");

    // First row: event_a1
    assert_eq!(rows[0].subject_id, event_a1.actor, "actor round-trips for A1");
    assert_eq!(rows[0].action, event_a1.action, "action round-trips for A1");
    assert_eq!(rows[0].reason_code, AuditStatus::Ok.as_str(), "status round-trips for A1");
    assert_eq!(rows[0].latency_ms, Some(12_i32), "latency_ms round-trips for A1");
    assert_eq!(rows[0].cost_micros, Some(500_i32), "cost_micros round-trips for A1");

    // Second row: event_a2
    assert_eq!(rows[1].subject_id, event_a2.actor, "actor round-trips for A2");
    assert_eq!(rows[1].action, event_a2.action, "action round-trips for A2");
    assert_eq!(rows[1].reason_code, AuditStatus::Ok.as_str(), "status round-trips for A2");
    assert_eq!(rows[1].latency_ms, Some(88_i32), "latency_ms round-trips for A2");
    assert_eq!(rows[1].cost_micros, None, "None cost_micros round-trips for A2");

    // -----------------------------------------------------------------------
    // 8. Clean up — admin pool bypasses RLS.
    // -----------------------------------------------------------------------
    sqlx::query("DELETE FROM audit_log WHERE tenant_id IN ($1, $2)")
        .bind(tenant_a)
        .bind(tenant_b)
        .execute(&admin_pool)
        .await
        .expect("cleanup audit rows");

    sqlx::query("DELETE FROM tenants WHERE id IN ($1, $2)")
        .bind(tenant_a)
        .bind(tenant_b)
        .execute(&admin_pool)
        .await
        .expect("cleanup tenants");
}

// ---------------------------------------------------------------------------
// Smoke: recorder gracefully returns Err on bad DSN (no panic)
// ---------------------------------------------------------------------------

/// Connecting to a non-existent database must return an error, not panic.
#[tokio::test]
async fn recorder_new_from_url_bad_dsn_returns_err() {
    // sqlx PgPool::connect fails fast on a clearly invalid host.
    let result =
        AuditRecorder::new_from_url("postgres://nobody:x@127.0.0.1:1/nonexistent").await;
    assert!(result.is_err(), "bad DSN must yield an error, not panic");
}
