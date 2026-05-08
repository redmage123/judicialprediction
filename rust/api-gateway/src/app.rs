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

use anyhow::Result;
use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema, SimpleObject};
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

struct Query;

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
}

// ---------------------------------------------------------------------------
// Application state — owns the GraphQL schema, JWT secret, and rate-limit store
// ---------------------------------------------------------------------------

type AppSchema = Schema<Query, EmptyMutation, EmptySubscription>;

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

    let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
        .data(fs_client)
        .data(audit_recorder)
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
