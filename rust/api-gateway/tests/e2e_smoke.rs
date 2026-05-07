// E2E smoke test: api-gateway → feature-store-server (gRPC) → Postgres with RLS.
//
// Requires:
//   - Docker Compose dev stack running (docker compose -f docker-compose.dev.yml up -d postgres)
//   - DATABASE_URL set to jp_app DSN, OR defaults to the dev stack DSN
//   - ADMIN_DATABASE_URL (optional) for test setup; falls back to the superuser DSN
//
// All tests are marked #[ignore] so they don't run in CI by default.
// Run explicitly with:
//   cargo test -p api-gateway --test e2e_smoke -- --include-ignored
//
// The tests prove the full path:
//   HTTP client → api-gateway (axum/GraphQL/JWT) → feature-store-server (tonic/gRPC) → Postgres RLS

use axum::body::Body;
use axum::http::{Request, StatusCode};
use feature_store::{
    judicialpredict::data_plane::feature_store::v1::feature_store_service_server::FeatureStoreServiceServer,
    server::FeatureStoreServer,
};
use tower::ServiceExt as _;

// ---------------------------------------------------------------------------
// Test constants
// ---------------------------------------------------------------------------

/// HS256 secret used for all test JWTs.  Never used outside of test code.
const TEST_JWT_SECRET: &[u8] = b"judicialpredict-test-jwt-secret!";

/// Dev tenant seeded by migration 20260507120001.
const DEV_TENANT: &str = "00000000-0000-0000-0000-000000000001";
/// A second UUID that has no data — used to probe RLS isolation.
const OTHER_TENANT: &str = "00000000-0000-0000-0000-000000000002";

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

/// Mint a valid HS256 test JWT for the given tenant UUID.
///
/// The token has a 1-hour expiry and a `features:read` scope.
fn make_jwt(tenant_id: &str) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;

    // Matches the Claims struct in auth.rs without importing it.
    #[derive(serde::Serialize)]
    struct TestClaims {
        sub: String,
        tenant_id: String,
        scopes: Vec<String>,
        exp: usize,
        iat: usize,
    }

    let claims = TestClaims {
        sub: "e2e-test-subject".to_string(),
        tenant_id: tenant_id.to_string(),
        scopes: vec!["features:read".to_string()],
        exp: now + 3600,
        iat: now,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(TEST_JWT_SECRET),
    )
    .expect("test JWT encoding failed")
}

/// Mint an already-expired HS256 JWT for the given tenant UUID.
fn make_expired_jwt(tenant_id: &str) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;

    #[derive(serde::Serialize)]
    struct TestClaims {
        sub: String,
        tenant_id: String,
        scopes: Vec<String>,
        exp: usize,
        iat: usize,
    }

    let claims = TestClaims {
        sub: "e2e-test-subject".to_string(),
        tenant_id: tenant_id.to_string(),
        scopes: vec![],
        exp: now - 3600, // 1 hour in the past
        iat: now - 7200,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(TEST_JWT_SECRET),
    )
    .expect("expired JWT encoding failed")
}

/// Spawn the feature-store gRPC server on a random free port.
/// Returns the URL and a join handle (abort to shut down).
async fn spawn_feature_store(pool: sqlx::PgPool) -> (String, tokio::task::JoinHandle<()>) {
    // Bind to a random free port so parallel test runs don't collide.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    let handle = tokio::spawn(async move {
        let server = FeatureStoreServer::new(pool);
        tonic::transport::Server::builder()
            .add_service(FeatureStoreServiceServer::new(server))
            .serve_with_incoming(incoming)
            .await
            .expect("feature-store-server failed");
    });

    // Give tonic a moment to bind.
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    (url, handle)
}

/// Helper: send a GraphQL query through the axum router (in-process, no TCP).
///
/// `jwt` should be a Bearer token string (e.g. from `make_jwt`).
/// Pass `""` to send a request with no Authorization header (for testing 401 paths).
async fn graphql(app: &axum::Router, query: &str, jwt: &str) -> (StatusCode, bytes::Bytes) {
    let body = format!(r#"{{"query": "{}"}}"#, query.replace('"', "\\\""));
    let mut builder = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json");

    if !jwt.is_empty() {
        builder = builder.header("authorization", format!("Bearer {jwt}"));
    }

    let req = builder.body(Body::from(body)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, body_bytes)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// S2.2 E2E smoke: api-gateway (JWT auth) → feature-store-server (gRPC) → Postgres with RLS.
///
/// 1. Starts feature-store-server on a random port.
/// 2. Builds api-gateway with the test JWT secret, pointing at that port.
/// 3. Inserts a feature for the dev tenant via the admin pool (bypasses RLS).
/// 4. Queries `feature(id: ...)` via GraphQL with a valid JWT for the correct tenant.
///    Expects 200 and the feature name in the response body.
/// 5. Repeats with a JWT for a DIFFERENT tenant.
///    Expects 200 with `data.feature = null` — RLS in the feature-store blocks the read.
/// 6. Cleans up.
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn graphql_feature_rls_smoke() {
    // -----------------------------------------------------------------------
    // 1. Start feature-store gRPC server on a random port.
    // -----------------------------------------------------------------------
    let jp_pool = sqlx::PgPool::connect(&jp_app_url())
        .await
        .expect("jp_app pool");
    let (fs_url, _fs_handle) = spawn_feature_store(jp_pool).await;

    // -----------------------------------------------------------------------
    // 2. Build the api-gateway with the test JWT secret.
    // -----------------------------------------------------------------------
    let app = api_gateway::build_app(&fs_url, TEST_JWT_SECRET.to_vec())
        .await
        .expect("build_app");

    // -----------------------------------------------------------------------
    // 3. Insert a test feature as the dev tenant via the admin pool (bypasses RLS).
    // -----------------------------------------------------------------------
    let admin_pool = sqlx::PgPool::connect(&admin_url())
        .await
        .expect("admin pool");
    let dev_tenant: uuid::Uuid = DEV_TENANT.parse().unwrap();

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
    // 4. Query with the CORRECT tenant JWT — expect the feature to be returned.
    // -----------------------------------------------------------------------
    let dev_jwt = make_jwt(DEV_TENANT);
    let gql = format!("{{ feature(id: \"{feature_id}\") {{ id name tier }} }}");
    let (status, body) = graphql(&app, &gql, &dev_jwt).await;

    assert_eq!(status, StatusCode::OK, "graphql must return 200");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("parse JSON");
    let returned_name = json["data"]["feature"]["name"].as_str().unwrap_or("");
    assert_eq!(
        returned_name, "e2e.smoke.judge.win_rate",
        "correct tenant must see the feature; body={body:?}"
    );

    // -----------------------------------------------------------------------
    // 5. Query with a DIFFERENT tenant JWT — RLS must block the read.
    // -----------------------------------------------------------------------
    let other_jwt = make_jwt(OTHER_TENANT);
    let (status2, body2) = graphql(&app, &gql, &other_jwt).await;

    assert_eq!(status2, StatusCode::OK, "graphql must still return 200");
    let json2: serde_json::Value = serde_json::from_slice(&body2).expect("parse JSON 2");
    // RLS makes the feature invisible to the other tenant — resolver returns null.
    assert!(
        json2["data"]["feature"].is_null(),
        "other tenant must NOT see the feature — RLS violation; body={body2:?}"
    );

    // -----------------------------------------------------------------------
    // 6. Cleanup.
    // -----------------------------------------------------------------------
    sqlx::query("DELETE FROM features WHERE id = $1")
        .bind(feature_id)
        .execute(&admin_pool)
        .await
        .expect("cleanup");
}

/// When the feature-store-server is not listening, the GraphQL resolver must
/// return a structured error (not panic or return 500).
#[tokio::test]
#[ignore = "requires docker-compose dev stack (for api-gateway startup); run with --include-ignored"]
async fn feature_store_grpc_unavailable_returns_error() {
    // Point api-gateway at a port with nothing listening.
    let dead_url = "http://127.0.0.1:49999";
    let app = api_gateway::build_app(dead_url, TEST_JWT_SECRET.to_vec())
        .await
        .expect("build_app");

    let dev_jwt = make_jwt(DEV_TENANT);
    let gql = r#"{ feature(id: "00000000-0000-0000-0000-000000000099") { id name } }"#;
    let (status, body) = graphql(&app, gql, &dev_jwt).await;

    // GraphQL always returns HTTP 200 even when resolvers error.
    assert_eq!(status, StatusCode::OK, "must return 200 even when feature-store is down");

    let json: serde_json::Value = serde_json::from_slice(&body).expect("parse JSON");

    // The response must have an `errors` array with at least one entry.
    assert!(
        json["errors"].is_array() && !json["errors"].as_array().unwrap().is_empty(),
        "errors array must be non-empty when feature-store is unavailable; body={body:?}"
    );
    let err_msg = json["errors"][0]["message"].as_str().unwrap_or("");
    assert!(
        err_msg.contains("feature-store unavailable"),
        "error message must indicate feature-store is unavailable; got: {err_msg}"
    );
}

/// Verify the health endpoint returns 200 without any auth header.
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn health_endpoint_ok() {
    // Feature-store doesn't need to be up for the health endpoint.
    let app = api_gateway::build_app("http://127.0.0.1:49998", TEST_JWT_SECRET.to_vec())
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

/// Verify that a missing Authorization header returns HTTP 401.
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn missing_tenant_header_is_unauthorized() {
    let app = api_gateway::build_app("http://127.0.0.1:49997", TEST_JWT_SECRET.to_vec())
        .await
        .expect("build_app");

    // `graphql` helper with an empty jwt string omits the Authorization header.
    let (status, _) = graphql(&app, "{ healthcheck }", "").await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "missing Authorization header must yield 401"
    );
}

/// Verify that a missing or expired JWT returns HTTP 401.
///
/// Tests three sub-cases:
///   a) No Authorization header at all.
///   b) A syntactically valid but expired JWT.
///   c) A garbage token that fails signature verification.
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn missing_or_expired_jwt_returns_401() {
    let app = api_gateway::build_app("http://127.0.0.1:49996", TEST_JWT_SECRET.to_vec())
        .await
        .expect("build_app");

    // a) No Authorization header.
    let (status_a, _) = graphql(&app, "{ healthcheck }", "").await;
    assert_eq!(status_a, StatusCode::UNAUTHORIZED, "no auth header → 401");

    // b) Expired JWT.
    let expired = make_expired_jwt(DEV_TENANT);
    let (status_b, _) = graphql(&app, "{ healthcheck }", &expired).await;
    assert_eq!(status_b, StatusCode::UNAUTHORIZED, "expired JWT → 401");

    // c) Garbage token (invalid signature).
    let (status_c, _) = graphql(&app, "{ healthcheck }", "garbage.token.here").await;
    assert_eq!(status_c, StatusCode::UNAUTHORIZED, "invalid token → 401");
}
