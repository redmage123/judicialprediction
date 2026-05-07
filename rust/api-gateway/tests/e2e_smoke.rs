// E2E smoke test: api-gateway → feature-store → Postgres with RLS
//
// Requires:
//   - Docker Compose dev stack running (docker compose -f docker-compose.dev.yml up -d postgres)
//   - DATABASE_URL set to jp_app DSN, OR defaults to the dev stack DSN
//   - ADMIN_DATABASE_URL (optional) for test setup; falls back to the superuser DSN
//
// All tests are marked #[ignore] so they don't run in CI by default.
// Run explicitly with:
//   cargo test -p api-gateway e2e -- --include-ignored
//
// The tests prove the full path:
//   HTTP client → api-gateway (axum) → feature-store (sqlx) → Postgres RLS

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt as _;

/// Dev tenant seeded by migration 20260507120001.
const DEV_TENANT: &str = "00000000-0000-0000-0000-000000000001";
/// A second UUID that has no data — used to probe RLS isolation.
const OTHER_TENANT: &str = "00000000-0000-0000-0000-000000000002";

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

/// Helper: execute a GraphQL query and return the response body bytes.
async fn graphql(
    app: &axum::Router,
    query: &str,
    tenant_id: &str,
) -> (StatusCode, bytes::Bytes) {
    let body = format!(r#"{{"query": "{}"}}"#, query.replace('"', "\\\""));
    let req = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("x-tenant-id", tenant_id)
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, body_bytes)
}

/// S1.11 E2E smoke: happy path + RLS isolation.
///
/// 1. Inserts a feature for the dev tenant via the feature-store crate
///    (bypassing the HTTP layer for setup, using the superuser pool).
/// 2. Queries `feature(id: ...)` via GraphQL with the correct tenant header.
///    Expects 200 and the feature name in the response.
/// 3. Repeats the query with a different tenant header.
///    Expects 200 with `data.feature = null` — RLS blocks the read.
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn graphql_feature_rls_smoke() {
    // -----------------------------------------------------------------------
    // 1. Set up: insert a feature as the dev tenant using a superuser
    //    connection (bypasses RLS so we can insert without SET LOCAL).
    // -----------------------------------------------------------------------
    let admin_pool = sqlx::PgPool::connect(&admin_url())
        .await
        .expect("admin pool");
    let dev_tenant: uuid::Uuid = DEV_TENANT.parse().unwrap();

    // Insert directly — no RLS on superuser.
    let feature_id: uuid::Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO features (tenant_id, case_id, name, value, tier, sensitivity, source, lineage)
        VALUES ($1, NULL, $2, $3, 'TIER_A'::feature_tier, 'PUBLIC'::feature_sensitivity, 'e2e_smoke', '{}')
        RETURNING id
        "#,
    )
    .bind(dev_tenant)
    .bind("e2e.smoke.judge.win_rate")
    .bind(serde_json::json!({"rate": 0.65}))
    .fetch_one(&admin_pool)
    .await
    .expect("insert test feature");

    // -----------------------------------------------------------------------
    // 2. Build the app (connects as jp_app — RLS enforced).
    // -----------------------------------------------------------------------
    let app = api_gateway::build_app(&jp_app_url())
        .await
        .expect("build_app");

    // -----------------------------------------------------------------------
    // 3. Query with the CORRECT tenant — expect the feature to be returned.
    // -----------------------------------------------------------------------
    let gql = format!("{{ feature(id: \"{feature_id}\") {{ id name tier }} }}");
    let (status, body) = graphql(&app, &gql, DEV_TENANT).await;

    assert_eq!(status, StatusCode::OK, "graphql must return 200");

    let json: serde_json::Value = serde_json::from_slice(&body).expect("parse JSON");
    let returned_name = json["data"]["feature"]["name"].as_str().unwrap_or("");
    assert_eq!(
        returned_name, "e2e.smoke.judge.win_rate",
        "correct tenant must see the feature; body={body:?}"
    );

    // -----------------------------------------------------------------------
    // 4. Query with a DIFFERENT tenant — RLS must block; expect null.
    // -----------------------------------------------------------------------
    let (status2, body2) = graphql(&app, &gql, OTHER_TENANT).await;

    assert_eq!(status2, StatusCode::OK, "graphql must still return 200");
    let json2: serde_json::Value = serde_json::from_slice(&body2).expect("parse JSON 2");
    assert!(
        json2["data"]["feature"].is_null(),
        "other tenant must NOT see the feature — RLS violation; body={body2:?}"
    );

    // -----------------------------------------------------------------------
    // 5. Cleanup.
    // -----------------------------------------------------------------------
    sqlx::query("DELETE FROM features WHERE id = $1")
        .bind(feature_id)
        .execute(&admin_pool)
        .await
        .expect("cleanup");
}

/// Verify the health endpoint returns 200 without any auth header.
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn health_endpoint_ok() {
    let app = api_gateway::build_app(&jp_app_url())
        .await
        .expect("build_app");

    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Verify that a missing X-Tenant-Id header returns HTTP 401.
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn missing_tenant_header_is_unauthorized() {
    let app = api_gateway::build_app(&jp_app_url())
        .await
        .expect("build_app");

    let req = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        // No X-Tenant-Id header.
        .body(Body::from(r#"{"query":"{ healthcheck }"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "missing X-Tenant-Id must yield 401"
    );
}
