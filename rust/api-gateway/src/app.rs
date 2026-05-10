// JudicialPredict API Gateway — application core.
//
// All GraphQL types, handlers, and the `build_app` factory live here.
// `src/lib.rs` re-exports `build_app`; `src/main.rs` just binds the
// TCP listener and calls into the library.
//
// SECURITY: every request to /graphql must carry an `Authorization: Bearer
// <jwt>` header.  The JWT middleware (rate_limit::jwt_middleware) decodes and
// verifies the token, then injects TenantId + Claims into request extensions.
// The rate-limit middleware (rate_limit::rate_limit_middleware) consumes one
// per-tenant token and returns 429 on exhaustion.  Both middlewares run only
// on the /graphql route; /health is unauthenticated.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context as _, Result};
use async_graphql::{Context, EmptySubscription, Object, Schema, SimpleObject};
use crate::graphql_predict::{
    CaseConnection, MlInferenceClient, Mutation, compute_next_offset,
};
use sqlx::PgPool;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use audit_recorder::{AuditEvent, AuditRecorder, AuditStatus, hash_payload};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use feature_store::judicialpredict::data_plane::feature_store::v1::{
    feature_store_service_client::FeatureStoreServiceClient, GetFeatureRequest,
};
use tonic::transport::Channel;
use uuid::Uuid;

use crate::auth::Claims;
use crate::rate_limit::{MemoryStore, RateLimitConfig, RateLimitStore};

// ---------------------------------------------------------------------------
// Tenant identity — injected by jwt_middleware; read by resolvers + rate-limit
// ---------------------------------------------------------------------------

/// Wraps the tenant UUID extracted from the validated JWT `tenant_id` claim.
#[derive(Clone, Copy)]
pub(crate) struct TenantId(pub(crate) Uuid);

// ---------------------------------------------------------------------------
// GraphQL data transfer objects
// ---------------------------------------------------------------------------

/// A feature as returned by the GraphQL API.
#[derive(SimpleObject)]
struct FeatureDto {
    /// Storage primary key (UUID, used as the stable feature_id in Sprint 1).
    id: String,
    /// Stable feature identifier, e.g. "judge.reversal_rate.circuit9".
    name: String,
    /// JSON-encoded feature value.
    value_json: String,
    /// Compliance tier: "TIER_A" | "TIER_B" | "TIER_C" | "TIER_D".
    tier: String,
    /// Sensitivity: "PUBLIC" | "QUASI_PUBLIC" | "INFERRED" | "PROTECTED".
    sensitivity: String,
    /// Case UUID, if the feature is case-scoped.
    case_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Tier/sensitivity i32 → string helpers
// ---------------------------------------------------------------------------

/// Map the proto wire integer back to the SQL tier enum string.
fn tier_to_str(i: i32) -> String {
    match i {
        1 => "TIER_A",
        2 => "TIER_B",
        3 => "TIER_C",
        4 => "TIER_D",
        _ => "TIER_UNSPECIFIED",
    }
    .to_string()
}

/// Map the proto wire integer back to the SQL sensitivity enum string.
fn sensitivity_to_str(i: i32) -> String {
    match i {
        1 => "PUBLIC",
        2 => "QUASI_PUBLIC",
        3 => "INFERRED",
        4 => "PROTECTED",
        _ => "SENSITIVITY_UNSPECIFIED",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// GraphQL schema — Query root
// ---------------------------------------------------------------------------

pub(crate) struct Query;

#[Object]
impl Query {
    /// Liveness check — always returns "ok".
    async fn healthcheck(&self) -> &str {
        "ok"
    }

    /// Look up a single feature by its storage UUID.
    ///
    /// Returns `null` if the feature does not exist or belongs to a
    /// different tenant (RLS enforces isolation transparently in feature-store).
    ///
    /// Returns a GraphQL error if the feature-store gRPC service is unavailable.
    ///
    /// Requires a valid `Authorization: Bearer <jwt>` header on the HTTP request.
    ///
    /// Every call is recorded in the `audit_log` table via a fire-and-forget
    /// `tokio::spawn` so the hot path is not blocked by the audit INSERT.
    async fn feature(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> async_graphql::Result<Option<FeatureDto>> {
        let start = Instant::now();

        let TenantId(tenant_id) = *ctx.data::<TenantId>().map_err(|_| "missing tenant id")?;

        // Clone the client — tonic clients are cheap to clone (shared channel).
        let mut client = ctx
            .data::<FeatureStoreServiceClient<Channel>>()
            .map_err(|_| "missing feature-store client")?
            .clone();

        // Attach tenant-id to the outbound gRPC request metadata.
        let mut request = tonic::Request::new(GetFeatureRequest {
            // Sprint 1: feature_id is the DB UUID.
            feature_id: id.clone(),
            case_id: String::new(),
            permitted_use: 0,
        });
        let tenant_val: tonic::metadata::MetadataValue<tonic::metadata::Ascii> = tenant_id
            .to_string()
            .parse()
            .map_err(|_| "invalid tenant id format")?;
        request.metadata_mut().insert("tenant-id", tenant_val);

        // Serialize the request fields for the audit payload hash (privacy-preserving).
        // Only the feature_id is hashed; the full payload is never stored.
        let payload_bytes = id.as_bytes();

        let result = client.get_feature(request).await;

        let latency_ms = start.elapsed().as_millis().min(u32::MAX as u128) as u32;

        let (status, grpc_result) = match &result {
            Ok(_) => (AuditStatus::Ok, result),
            Err(s) if s.code() == tonic::Code::ResourceExhausted => {
                (AuditStatus::RateLimit, result)
            }
            Err(s) if s.code() == tonic::Code::DeadlineExceeded => {
                (AuditStatus::Timeout, result)
            }
            Err(_) => (AuditStatus::Err, result),
        };

        // Fire-and-forget audit record.  Failure is intentionally swallowed —
        // audit recording must never block or fail the request.
        if let Some(recorder) = ctx.data::<Option<AuditRecorder>>().ok().and_then(|r| r.as_ref()) {
            let recorder = recorder.clone();
            let event = AuditEvent {
                actor: "api-gateway".to_string(),
                action: "feature_store.GetFeature".to_string(),
                payload_hash: hash_payload(payload_bytes),
                latency_ms,
                status,
                cost_micros: None, // gRPC call; no per-call token cost
            };
            tokio::spawn(async move {
                if let Err(e) = recorder.record(tenant_id, event).await {
                    tracing::warn!(error = %e, "audit record failed (non-fatal)");
                }
            });
        }

        let response = grpc_result.map_err(|status| {
            async_graphql::Error::new(format!(
                "feature-store unavailable: {} {}",
                status.code(),
                status.message()
            ))
        })?;

        let feature = response.into_inner().feature;
        Ok(feature.map(|f| FeatureDto {
            id: f.feature_id,
            name: f.name,
            value_json: f.value_json,
            tier: tier_to_str(f.tier),
            sensitivity: sensitivity_to_str(f.sensitivity),
            case_id: if f.case_id.is_empty() {
                None
            } else {
                Some(f.case_id)
            },
        }))
    }

    /// List persisted cases for the current tenant, paginated.
    ///
    /// Returns cases ordered by `created_at DESC`.  Tenant isolation is
    /// enforced by both an explicit `WHERE tenant_id = $1` clause and by
    /// `SET LOCAL app.current_tenant_id` (RLS belt-and-suspenders, mirroring
    /// the audit-recorder pattern).
    ///
    /// Valid ranges: `limit ∈ [1, 100]`, `offset ≥ 0`.  Values outside
    /// these ranges return a GraphQL error without touching the database.
    ///
    /// Returns a `CaseConnection` containing the page nodes, the tenant-wide
    /// `totalCount`, and a `nextOffset` cursor (`null` on the last page).
    ///
    /// Legacy cases that have NULL `input_features`/`prediction`/`recommendation`
    /// columns (inserted before S4.1) will produce a GraphQL error naming the
    /// offending case UUID — they do NOT silently degrade.
    async fn list_cases(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20)] limit: i32,
        #[graphql(default = 0)] offset: i32,
    ) -> async_graphql::Result<CaseConnection> {
        use async_graphql::{Json, ID};
        use crate::graphql_predict::{Case, PredictInput, PredictResult, RecommendationDto};
        use sqlx::Row as _;

        // Validate range before touching the DB.
        if !(1..=100).contains(&limit) {
            return Err(async_graphql::Error::new("limit must be between 1 and 100"));
        }
        if offset < 0 {
            return Err(async_graphql::Error::new("offset must be >= 0"));
        }

        let TenantId(tenant_id) = *ctx
            .data::<TenantId>()
            .map_err(|_| async_graphql::Error::new("missing tenant id"))?;

        let pool = ctx
            .data::<Option<Arc<PgPool>>>()
            .map_err(|_| async_graphql::Error::new("cases store unavailable"))?
            .as_ref()
            .ok_or_else(|| {
                async_graphql::Error::new(
                    "cases store not configured (DATABASE_URL missing)",
                )
            })?;

        let mut tx = pool
            .begin()
            .await
            .map_err(|e| async_graphql::Error::new(format!("cases tx begin: {e}")))?;

        // SET LOCAL so the RLS insert-policy is evaluated on this connection
        // even when it was previously pooled without the setting.
        // Uuid::to_string() is injection-safe.
        sqlx::query(&format!(
            "SET LOCAL app.current_tenant_id = '{tenant_id}'"
        ))
        .execute(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("SET LOCAL failed: {e}")))?;

        // 1. Total row count for pagination metadata.
        let total_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cases WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| async_graphql::Error::new(format!("count query failed: {e}")))?;

        // 2. Fetch the requested page, most-recent-first.
        let rows = sqlx::query(
            r#"
            SELECT id,
                   tenant_id,
                   input_features,
                   prediction,
                   recommendation,
                   created_by,
                   created_at::text AS created_at_s
            FROM   cases
            WHERE  tenant_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("list query failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| async_graphql::Error::new(format!("cases tx commit: {e}")))?;

        // 3. Map rows → Case objects, emitting a named GraphQL error for any
        //    row that has NULL jsonb columns (legacy rows predating S4.1).
        let mut nodes = Vec::with_capacity(rows.len());
        for row in &rows {
            let id: Uuid = row
                .try_get("id")
                .map_err(|e| async_graphql::Error::new(format!("row.id: {e}")))?;
            let tenant_id_col: Uuid = row
                .try_get("tenant_id")
                .map_err(|e| async_graphql::Error::new(format!("row.tenant_id: {e}")))?;
            let created_by: Option<Uuid> = row
                .try_get("created_by")
                .map_err(|e| async_graphql::Error::new(format!("row.created_by: {e}")))?;
            let created_at: String = row
                .try_get("created_at_s")
                .map_err(|e| async_graphql::Error::new(format!("row.created_at: {e}")))?;

            // NULL jsonb columns → named error so callers can identify the case.
            let input_features_val: serde_json::Value = row
                .try_get("input_features")
                .map_err(|e| {
                    async_graphql::Error::new(format!(
                        "case {id}: input_features is NULL (legacy row): {e}"
                    ))
                })?;
            let prediction_val: serde_json::Value = row
                .try_get("prediction")
                .map_err(|e| {
                    async_graphql::Error::new(format!(
                        "case {id}: prediction is NULL (legacy row): {e}"
                    ))
                })?;
            let recommendation_val: serde_json::Value = row
                .try_get("recommendation")
                .map_err(|e| {
                    async_graphql::Error::new(format!(
                        "case {id}: recommendation is NULL (legacy row): {e}"
                    ))
                })?;

            let input_features: PredictInput =
                serde_json::from_value(input_features_val).map_err(|e| {
                    async_graphql::Error::new(format!(
                        "case {id}: input_features parse error: {e}"
                    ))
                })?;
            let prediction: PredictResult =
                serde_json::from_value(prediction_val).map_err(|e| {
                    async_graphql::Error::new(format!(
                        "case {id}: prediction parse error: {e}"
                    ))
                })?;
            let recommendation: RecommendationDto =
                serde_json::from_value(recommendation_val).map_err(|e| {
                    async_graphql::Error::new(format!(
                        "case {id}: recommendation parse error: {e}"
                    ))
                })?;

            nodes.push(Case {
                id:             ID::from(id.to_string()),
                tenant_id:      ID::from(tenant_id_col.to_string()),
                input_features: Json(input_features),
                prediction,
                recommendation,
                created_by:     created_by.map(|u| ID::from(u.to_string())),
                created_at,
            });
        }

        let next_offset = compute_next_offset(offset, nodes.len(), total_count);
        Ok(CaseConnection { nodes, total_count, next_offset })
    }
}

// ---------------------------------------------------------------------------
// Application state — owns the GraphQL schema, JWT secret, and rate-limit store
// ---------------------------------------------------------------------------

type AppSchema = Schema<Query, Mutation, EmptySubscription>;

/// Shared application state injected into every axum handler via `State<Arc<AppState>>`.
pub(crate) struct AppState {
    pub(crate) schema: AppSchema,
    /// HS256 secret bytes. In dev, read from `JWT_SECRET` env var or the test
    /// constant. In prod, injected from External Secrets Operator (Sprint 3+).
    pub(crate) jwt_secret: Vec<u8>,
    /// Per-tenant token-bucket store. In-memory for now; Redis-backed in prod.
    pub(crate) rate_store: Arc<dyn RateLimitStore>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Serves the GraphQL endpoint.
///
/// JWT validation and rate-limiting are handled by upstream middleware layers.
/// By the time this handler is reached, `TenantId` and `Claims` are guaranteed
/// to be present in request extensions.
async fn graphql_handler(
    State(state): State<Arc<AppState>>,
    Extension(tenant_id): Extension<TenantId>,
    Extension(claims): Extension<Claims>,
    req: GraphQLRequest,
) -> Result<GraphQLResponse, StatusCode> {
    let gql_req = req
        .into_inner()
        .data(tenant_id)
        .data(claims);

    Ok(state.schema.execute(gql_req).await.into())
}

/// Simple HTTP liveness probe — does not require authentication.
async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

// ---------------------------------------------------------------------------
// App builder — exported via lib.rs for use in tests
// ---------------------------------------------------------------------------

/// Build the axum `Router`, wiring the GraphQL schema, JWT middleware,
/// per-tenant rate-limit middleware, and health endpoint.
///
/// # Parameters
/// - `feature_store_grpc_url` — gRPC endpoint for the feature-store service.
/// - `ml_inference_url` — HTTP base URL for ml-inference-svc
///   (e.g. `http://ml-inference-svc:8001`).  Set via `ML_INFERENCE_URL`.
///   Sprint-4 follow-up: replace with gRPC once the Python svc exposes a
///   gRPC server (protos/ml_plane/inference.proto).
/// - `jwt_secret` — raw bytes of the HS256 signing secret.
/// - `rate_config` — rate-limiting parameters (RPM caps per tenant).
///
/// # Audit recording
/// Reads `AUDIT_DATABASE_URL` (falls back to `DATABASE_URL`) from the
/// environment.  If neither is set, or the connection fails, audit recording
/// is silently disabled — the gateway remains fully functional.
///
/// # Middleware stack (applied to `/graphql` only)
/// ```text
/// jwt_middleware (outermost) → rate_limit_middleware → graphql_handler
/// ```
pub async fn build_app(
    feature_store_grpc_url: &str,
    ml_inference_url: &str,
    jwt_secret: Vec<u8>,
    rate_config: RateLimitConfig,
) -> Result<Router> {
    let channel = Channel::from_shared(feature_store_grpc_url.to_string())
        .expect("invalid feature-store URL")
        .connect_lazy();

    let fs_client = FeatureStoreServiceClient::new(channel);

    // Optionally connect an audit recorder.  Non-fatal: missing env var or
    // unreachable DB just means audit events are dropped with a warning.
    let audit_recorder: Option<AuditRecorder> =
        match std::env::var("AUDIT_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
        {
            Ok(url) => match AuditRecorder::new_from_url(&url).await {
                Ok(r) => {
                    tracing::info!("audit recorder connected");
                    Some(r)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "audit recorder unavailable — recording disabled");
                    None
                }
            },
            Err(_) => {
                tracing::debug!("AUDIT_DATABASE_URL / DATABASE_URL not set — audit recording disabled");
                None
            }
        };

    // Cases pool for createCase / listCases resolvers.
    // Connects with the jp_app role (DATABASE_URL) so RLS policies are active.
    // Non-fatal: missing or unreachable DB disables the two mutations silently.
    let cases_pool: Option<Arc<PgPool>> = match std::env::var("DATABASE_URL") {
        Ok(url) => match sqlx::PgPool::connect(&url).await {
            Ok(pool) => {
                tracing::info!("cases pool connected");
                Some(Arc::new(pool))
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "cases pool unavailable — createCase/listCases disabled"
                );
                None
            }
        },
        Err(_) => {
            tracing::debug!("DATABASE_URL not set — cases pool disabled");
            None
        }
    };

    // HTTP client for ml-inference-svc POST /predict (Sprint-3 HTTP shortcut).
    // Timeouts: 10 s connect (fail-fast if service is unreachable),
    //           30 s total   (conformal CI can take a few seconds on cold model load).
    let ml_http_client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("build ml-inference reqwest client")?;

    let ml_client = MlInferenceClient {
        client: ml_http_client,
        base_url: ml_inference_url.trim_end_matches('/').to_string(),
    };

    let schema = Schema::build(Query, Mutation, EmptySubscription)
        .data(fs_client)
        .data(audit_recorder)
        .data(ml_client)
        .data(cases_pool)
        .finish();

    let rate_store: Arc<dyn RateLimitStore> =
        Arc::new(MemoryStore::new(rate_config.requests_per_min));

    let state = Arc::new(AppState { schema, jwt_secret, rate_store });

    // Build the /graphql sub-router with both auth + rate-limit middlewares.
    // route_layer is applied in reverse declaration order (last = outermost):
    //   jwt_middleware   ← outermost (runs first; injects extensions; 401 on failure)
    //   rate_limit_middleware ← inner (runs second; 429 on exhaustion)
    let graphql_router = Router::new()
        .route("/graphql", post(graphql_handler))
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            crate::rate_limit::rate_limit_middleware,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            crate::rate_limit::jwt_middleware,
        ));

    let app = Router::new()
        .route("/health", get(health_handler))
        .merge(graphql_router)
        .with_state(state);

    Ok(app)
}
