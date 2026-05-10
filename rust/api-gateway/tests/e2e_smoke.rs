// E2E smoke test: api-gateway → feature-store-server (gRPC) → Postgres with RLS.
//
// Requires (for feature-store tests):
//   - Docker Compose dev stack running (docker compose -f docker-compose.dev.yml up -d postgres)
//   - DATABASE_URL set to jp_app DSN, OR defaults to the dev stack DSN
//   - ADMIN_DATABASE_URL (optional) for test setup; falls back to the superuser DSN
//
// predict_mutation_happy_path requires only a wiremock server (no docker stack).
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
    tenant_settings::OverridesCache,
};
use tower::ServiceExt as _;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
        // Test fixtures: empty overrides cache + a real audit-recorder bound
        // to the same pool. The recorder writes through; tests don't assert on
        // audit_log rows (S3.11 covers that).
        let overrides_cache = OverridesCache::new();
        let recorder = audit_recorder::AuditRecorder::new(pool.clone());
        let server = FeatureStoreServer::new(pool, overrides_cache, recorder);
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
    let app = api_gateway::build_app(
        &fs_url,
        "http://127.0.0.1:49990", // ml-inference not used in this test
        TEST_JWT_SECRET.to_vec(),
        Default::default(),
    )
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
    let app = api_gateway::build_app(
        dead_url,
        "http://127.0.0.1:49990", // ml-inference not used in this test
        TEST_JWT_SECRET.to_vec(),
        Default::default(),
    )
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
    let app = api_gateway::build_app(
        "http://127.0.0.1:49998",
        "http://127.0.0.1:49990",
        TEST_JWT_SECRET.to_vec(),
        Default::default(),
    )
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
    let app = api_gateway::build_app(
        "http://127.0.0.1:49997",
        "http://127.0.0.1:49990",
        TEST_JWT_SECRET.to_vec(),
        Default::default(),
    )
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
    let app = api_gateway::build_app(
        "http://127.0.0.1:49996",
        "http://127.0.0.1:49990",
        TEST_JWT_SECRET.to_vec(),
        Default::default(),
    )
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

/// S3.1 predict mutation happy path — uses a wiremock mock server; no docker stack required.
///
/// 1. Starts a wiremock server that expects exactly one POST /predict with the
///    correct X-Tenant-Id header and returns a fixed prediction JSON.
/// 2. Builds api-gateway pointing at the mock server for ML inference.
/// 3. Issues a JWT for DEV_TENANT.
/// 4. POSTs the `predictCaseOutcome` GraphQL mutation.
/// 5. Asserts the response decodes to the expected PredictResult fields.
/// 6. On mock server drop, wiremock verifies the expect(1) constraint was met
///    (i.e., the request with X-Tenant-Id header was received exactly once).
///
/// Audit row assertion is in S3.11 — out of scope here.
#[tokio::test]
#[ignore = "boots a wiremock mock server; run with --include-ignored"]
async fn predict_mutation_happy_path() {
    // -----------------------------------------------------------------------
    // 1. Start the mock ml-inference-svc.
    // -----------------------------------------------------------------------
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/predict"))
        // Verify the gateway threads the JWT tenant_id into X-Tenant-Id.
        .and(header("X-Tenant-Id", DEV_TENANT))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "p_win":            0.72_f64,
                "ci_lower":         0.61_f64,
                "ci_upper":         0.83_f64,
                "coverage":         0.90_f64,
                "model_version":    "test-run-id-abc123",
                "predicted_at_unix": 1_746_748_800_i64
            })),
        )
        // Dropping the MockServer verifies this expectation automatically.
        .expect(1)
        .mount(&mock_server)
        .await;

    // -----------------------------------------------------------------------
    // 2. Build api-gateway pointing at the mock server.
    //    Feature-store URL is unused; gRPC channel is lazy.
    // -----------------------------------------------------------------------
    let app = api_gateway::build_app(
        "http://127.0.0.1:49993",
        &mock_server.uri(),
        TEST_JWT_SECRET.to_vec(),
        Default::default(),
    )
    .await
    .expect("build_app");

    // -----------------------------------------------------------------------
    // 3. Mint a JWT for the dev tenant.
    // -----------------------------------------------------------------------
    let jwt = make_jwt(DEV_TENANT);

    // -----------------------------------------------------------------------
    // 4. POST the GraphQL mutation.
    //    Note: async-graphql converts snake_case fields to camelCase in SDL.
    // -----------------------------------------------------------------------
    let gql_body = serde_json::json!({
        "query": r#"
            mutation {
                predictCaseOutcome(input: {
                    judgeSeverity:         0.7
                    attorneyWinRate:       0.6
                    ideologyDistance:      0.3
                    materialityScore:      0.8
                    proceduralMotionCount: 3.0
                    caseType:              "civil"
                    jurisdiction:          "Federal"
                }) {
                    pWin
                    ciLower
                    ciUpper
                    coverage
                    modelVersion
                    predictedAtUnix
                }
            }
        "#
    })
    .to_string();

    let req = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {jwt}"))
        .body(Body::from(gql_body))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();

    // -----------------------------------------------------------------------
    // 5. Assert the response decodes correctly.
    // -----------------------------------------------------------------------
    assert_eq!(resp.status(), StatusCode::OK, "GraphQL must return 200");

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("parse JSON");

    // Must have no top-level errors.
    assert!(
        json["errors"].is_null(),
        "response must have no errors; body={body_bytes:?}"
    );

    let result = &json["data"]["predictCaseOutcome"];
    assert!(
        (result["pWin"].as_f64().unwrap_or(0.0) - 0.72).abs() < 1e-4,
        "pWin mismatch; body={body_bytes:?}"
    );
    assert!(
        (result["ciLower"].as_f64().unwrap_or(0.0) - 0.61).abs() < 1e-4,
        "ciLower mismatch"
    );
    assert!(
        (result["ciUpper"].as_f64().unwrap_or(0.0) - 0.83).abs() < 1e-4,
        "ciUpper mismatch"
    );
    assert_eq!(
        result["modelVersion"].as_str().unwrap_or(""),
        "test-run-id-abc123",
        "modelVersion mismatch"
    );
    assert_eq!(
        result["predictedAtUnix"].as_i64().unwrap_or(0),
        1_746_748_800_i64,
        "predictedAtUnix mismatch"
    );

    // 6. MockServer drop verifies expect(1) — the X-Tenant-Id header was received.
    //    No explicit assertion needed; wiremock panics if the expectation is unmet.
}

/// S4.2 E2E smoke: `createCase` persists a row to `cases` and fires an audit event.
///
/// 1. Starts a wiremock server that stubs POST /predict (no docker stack for ML).
/// 2. Builds api-gateway with DATABASE_URL pointing at the dev postgres.
/// 3. Sends the `createCase` mutation with a valid JWT for DEV_TENANT.
/// 4. Asserts the GraphQL response has a valid UUID id, correct tenantId, and
///    the expected pWin value from the mock ML response.
/// 5. Queries `cases` via the admin pool to confirm the row was persisted.
/// 6. Queries `audit_log` to confirm a `case.created` event was recorded.
/// 7. Cleans up the inserted row.
#[tokio::test]
#[ignore = "requires docker-compose dev stack + wiremock; run with --include-ignored"]
async fn create_case_persists_and_returns_with_audit() {
    // -----------------------------------------------------------------------
    // 1. Mock ml-inference-svc — no docker stack needed for this part.
    // -----------------------------------------------------------------------
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/predict"))
        .and(header("X-Tenant-Id", DEV_TENANT))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "p_win":             0.72_f64,
            "ci_lower":          0.61_f64,
            "ci_upper":          0.83_f64,
            "coverage":          0.90_f64,
            "model_version":     "test-run-id-e2e-create",
            "predicted_at_unix": 1_746_748_800_i64
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // -----------------------------------------------------------------------
    // 2. Build api-gateway — DATABASE_URL must be set for the cases pool.
    // -----------------------------------------------------------------------
    let app = api_gateway::build_app(
        "http://127.0.0.1:49993",
        &mock_server.uri(),
        TEST_JWT_SECRET.to_vec(),
        Default::default(),
    )
    .await
    .expect("build_app");

    let jwt = make_jwt(DEV_TENANT);

    // -----------------------------------------------------------------------
    // 3. Send the createCase mutation.
    // -----------------------------------------------------------------------
    let gql_body = serde_json::json!({
        "query": r#"
            mutation {
                createCase(input: {
                    judgeSeverity:         0.7
                    attorneyWinRate:       0.6
                    ideologyDistance:      0.3
                    materialityScore:      0.8
                    proceduralMotionCount: 3.0
                    caseType:              "civil"
                    jurisdiction:          "Federal"
                }) {
                    id
                    tenantId
                    prediction { pWin }
                    recommendation { kind expectedValueTry }
                    createdAt
                }
            }
        "#
    })
    .to_string();

    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {jwt}"))
        .body(axum::body::Body::from(gql_body))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();

    // -----------------------------------------------------------------------
    // 4. Assert the GraphQL response.
    // -----------------------------------------------------------------------
    assert_eq!(resp.status(), StatusCode::OK, "createCase must return HTTP 200");

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("parse JSON");

    assert!(
        json["errors"].is_null(),
        "createCase must have no errors; body={body_bytes:?}"
    );

    let case = &json["data"]["createCase"];
    let case_id_str = case["id"].as_str().expect("id must be a string");
    let case_id: uuid::Uuid = case_id_str.parse().expect("id must be a valid UUID");

    assert_eq!(
        case["tenantId"].as_str().unwrap_or(""),
        DEV_TENANT,
        "tenantId must equal the JWT tenant"
    );

    let p_win = case["prediction"]["pWin"].as_f64().unwrap_or(0.0);
    assert!(
        (p_win - 0.72).abs() < 1e-4,
        "pWin must match mock response; got {p_win}"
    );

    // recommendation.expectedValueTry must be a string (Decimal precision guard).
    assert!(
        case["recommendation"]["expectedValueTry"].is_string(),
        "expectedValueTry must be a JSON string, not a number"
    );

    // -----------------------------------------------------------------------
    // 5. Verify the row exists in the DB.
    // -----------------------------------------------------------------------
    let admin_pool = sqlx::PgPool::connect(&admin_url())
        .await
        .expect("admin pool");

    let row_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cases WHERE id = $1")
            .bind(case_id)
            .fetch_one(&admin_pool)
            .await
            .expect("DB count check");

    assert_eq!(row_count, 1, "one cases row must exist for the new case");

    // -----------------------------------------------------------------------
    // 6. Verify a case.created audit event was recorded.
    // -----------------------------------------------------------------------
    // Allow a brief moment for the fire-and-forget audit spawn to complete.
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE tenant_id = $1 AND action = 'case.created'",
    )
    .bind(uuid::Uuid::parse_str(DEV_TENANT).unwrap())
    .fetch_one(&admin_pool)
    .await
    .expect("audit count check");

    assert!(audit_count >= 1, "at least one case.created audit event must exist");

    // -----------------------------------------------------------------------
    // 7. Cleanup.
    // -----------------------------------------------------------------------
    sqlx::query("DELETE FROM cases WHERE id = $1")
        .bind(case_id)
        .execute(&admin_pool)
        .await
        .expect("cleanup");

    // MockServer drop verifies expect(1) — wiremock panics if unmet.
}

/// S4.3 E2E smoke: `listCases` paginates correctly and tenant-isolates rows via RLS.
///
/// 1. Seeds 3 cases for tenant A (DEV_TENANT) and 1 case for tenant B
///    (OTHER_TENANT) via the admin pool (bypasses RLS).
/// 2. Calls `listCases` as tenant A (default limit=20, offset=0).
///    Asserts ≥ 3 rows returned and tenant B's case UUID is never present.
/// 3. Calls `listCases` with limit=2, offset=0.
///    Asserts exactly 2 nodes are returned and `nextOffset = 2`.
/// 4. Cleans up all seeded rows.
#[tokio::test]
#[ignore = "requires docker-compose dev stack; run with --include-ignored"]
async fn list_cases_paginates_and_isolates_tenants() {
    let admin_pool = sqlx::PgPool::connect(&admin_url())
        .await
        .expect("admin pool");

    let tenant_a: uuid::Uuid = DEV_TENANT.parse().unwrap();
    let tenant_b: uuid::Uuid = OTHER_TENANT.parse().unwrap();

    // Reusable jsonb fixture values that satisfy the S4.2 column shapes.
    let features_val = serde_json::json!({
        "judge_severity": 0.5_f64,
        "attorney_win_rate": 0.5_f64,
        "ideology_distance": 0.5_f64,
        "materiality_score": 0.5_f64,
        "procedural_motion_count": 1.0_f64,
        "case_type": "civil",
        "jurisdiction": "Federal"
    });
    let prediction_val = serde_json::json!({
        "p_win": 0.5_f64,
        "ci_lower": 0.4_f64,
        "ci_upper": 0.6_f64,
        "coverage": 0.9_f64,
        "model_version": "e2e-seed",
        "predicted_at_unix": 1_746_748_800_i64
    });
    let recommendation_val = serde_json::json!({
        "kind": "Borderline",
        "rationale_bullets": ["b1", "b2", "b3"],
        "expected_value_try":    "5000.00",
        "expected_value_settle": "40000.00"
    });

    // -----------------------------------------------------------------------
    // 1. Seed 3 cases for tenant A and 1 for tenant B via admin pool.
    // -----------------------------------------------------------------------
    let mut case_ids: Vec<uuid::Uuid> = Vec::new();

    for _ in 0..3_u8 {
        let id: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO cases
                (tenant_id, title, jurisdiction, input_features, prediction, recommendation)
            VALUES ($1, 'E2E seed', 'Federal', $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(tenant_a)
        .bind(&features_val)
        .bind(&prediction_val)
        .bind(&recommendation_val)
        .fetch_one(&admin_pool)
        .await
        .expect("insert tenant-A seed case");

        case_ids.push(id);
    }

    let tenant_b_case_id: uuid::Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO cases
            (tenant_id, title, jurisdiction, input_features, prediction, recommendation)
        VALUES ($1, 'E2E seed B', 'Federal', $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(tenant_b)
    .bind(&features_val)
    .bind(&prediction_val)
    .bind(&recommendation_val)
    .fetch_one(&admin_pool)
    .await
    .expect("insert tenant-B seed case");

    // -----------------------------------------------------------------------
    // 2. Build app (DATABASE_URL must be set for the cases pool).
    //    Feature-store and ML inference URLs are never called in this test.
    // -----------------------------------------------------------------------
    let app = api_gateway::build_app(
        "http://127.0.0.1:49993",
        "http://127.0.0.1:49990",
        TEST_JWT_SECRET.to_vec(),
        Default::default(),
    )
    .await
    .expect("build_app");

    let jwt_a = make_jwt(DEV_TENANT);

    // -----------------------------------------------------------------------
    // 3. listCases as tenant A (defaults: limit=20, offset=0).
    // -----------------------------------------------------------------------
    let gql_all = serde_json::json!({
        "query": "{ listCases { nodes { id } totalCount nextOffset } }"
    })
    .to_string();

    let req_all = axum::http::Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {jwt_a}"))
        .body(axum::body::Body::from(gql_all))
        .unwrap();

    let resp_all = app.clone().oneshot(req_all).await.unwrap();
    let body_all = axum::body::to_bytes(resp_all.into_body(), usize::MAX)
        .await
        .unwrap();
    let json_all: serde_json::Value = serde_json::from_slice(&body_all).expect("parse JSON");

    assert!(
        json_all["errors"].is_null(),
        "listCases must have no errors; body={body_all:?}"
    );

    let result_all = &json_all["data"]["listCases"];
    let total = result_all["totalCount"].as_i64().unwrap_or(0);
    assert!(
        total >= 3,
        "tenant A must see at least 3 cases (seeded); got totalCount={total}"
    );

    // Tenant B's case must NOT appear in any page returned to tenant A.
    let tenant_b_str = tenant_b_case_id.to_string();
    let returned_ids: Vec<&str> = result_all["nodes"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|n| n["id"].as_str())
        .collect();

    assert!(
        !returned_ids.contains(&tenant_b_str.as_str()),
        "tenant A must NOT see tenant B's case (RLS isolation check)"
    );

    // -----------------------------------------------------------------------
    // 4. Pagination check: limit=2 → exactly 2 nodes + nextOffset = 2.
    // -----------------------------------------------------------------------
    let gql_limit2 = serde_json::json!({
        "query": "{ listCases(limit: 2, offset: 0) { nodes { id } totalCount nextOffset } }"
    })
    .to_string();

    let req_limit2 = axum::http::Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {jwt_a}"))
        .body(axum::body::Body::from(gql_limit2))
        .unwrap();

    let resp_limit2 = app.clone().oneshot(req_limit2).await.unwrap();
    let body_limit2 = axum::body::to_bytes(resp_limit2.into_body(), usize::MAX)
        .await
        .unwrap();
    let json_limit2: serde_json::Value =
        serde_json::from_slice(&body_limit2).expect("parse JSON limit=2");

    assert!(
        json_limit2["errors"].is_null(),
        "listCases(limit=2) must have no errors; body={body_limit2:?}"
    );

    let result_limit2 = &json_limit2["data"]["listCases"];
    let nodes_limit2 = result_limit2["nodes"].as_array().unwrap();
    assert_eq!(
        nodes_limit2.len(),
        2,
        "limit=2 must return exactly 2 nodes"
    );
    assert_eq!(
        result_limit2["nextOffset"].as_i64().unwrap_or(-1),
        2,
        "nextOffset must equal 2 after first page of 2 (total >= 3)"
    );

    // -----------------------------------------------------------------------
    // 5. Cleanup all seeded rows.
    // -----------------------------------------------------------------------
    for id in &case_ids {
        sqlx::query("DELETE FROM cases WHERE id = $1")
            .bind(id)
            .execute(&admin_pool)
            .await
            .ok();
    }
    sqlx::query("DELETE FROM cases WHERE id = $1")
        .bind(tenant_b_case_id)
        .execute(&admin_pool)
        .await
        .ok();
}

/// S2.3 rate-limit: per-tenant token bucket returns 429 after exhaustion.
///
/// Uses RPM=5 so only 6 requests are needed, keeping the test fast.
/// Runs fully in-process — `{ healthcheck }` never reaches the feature-store
/// or Postgres, so NO docker-compose stack is required.
///
/// Asserts:
///   - Requests 1-5 return HTTP 200.
///   - Request 6 returns HTTP 429 with a `Retry-After` header.
#[tokio::test]
async fn rate_limit_returns_429_after_exhaustion() {
    let rate_cfg = api_gateway::RateLimitConfig { requests_per_min: 5, mutations_per_min: 10 };
    // Feature-store and ML inference URLs are never called — channels are lazy.
    let app = api_gateway::build_app(
        "http://127.0.0.1:49994",
        "http://127.0.0.1:49990",
        TEST_JWT_SECRET.to_vec(),
        rate_cfg,
    )
    .await
    .expect("build_app");

    let jwt = make_jwt(DEV_TENANT);

    // First 5 requests must succeed (bucket starts at capacity = 5).
    for i in 0..5_u32 {
        let (status, _) = graphql(&app, "{ healthcheck }", &jwt).await;
        assert_eq!(status, StatusCode::OK, "request {i} should succeed — bucket not yet exhausted");
    }

    // 6th request: bucket empty → must be rate-limited.
    let body = format!(r#"{{"query": "{{ healthcheck }}"}}"#);
    let req = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {jwt}"))
        .body(axum::body::Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "6th request must return 429 when bucket is exhausted"
    );
    assert!(
        resp.headers().contains_key("retry-after"),
        "429 response must include a Retry-After header (RFC 9110 §10.2.4)"
    );
}
