// JudicialPredict API Gateway — application core.
//
// All GraphQL types, handlers, and the `build_app` factory live here.
// `src/lib.rs` re-exports `build_app`; `src/main.rs` just binds the
// TCP listener and calls into the library.
//
// SECURITY: every request to /graphql must carry an `Authorization: Bearer
// <jwt>` header. The middleware decodes and verifies the token; on success,
// the validated Claims (including tenant_id) are injected into the GraphQL
// request data so resolvers can attach the tenant UUID as gRPC metadata on
// every call to the feature-store-server, ensuring Postgres RLS evaluates the
// correct isolation policy.
//
// The /health endpoint is unauthenticated and always returns 200.

use anyhow::Result;
use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema, SimpleObject};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use feature_store::judicialpredict::data_plane::feature_store::v1::{
    feature_store_service_client::FeatureStoreServiceClient, GetFeatureRequest,
};
use std::sync::Arc;
use tonic::transport::Channel;
use uuid::Uuid;

use crate::auth::{decode_jwt, Claims};

// ---------------------------------------------------------------------------
// Tenant identity — injected per request into the GraphQL context
// ---------------------------------------------------------------------------

/// Wraps the tenant UUID extracted from the validated JWT `tenant_id` claim.
#[derive(Clone, Copy)]
struct TenantId(Uuid);

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
    async fn feature(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> async_graphql::Result<Option<FeatureDto>> {
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

        let response = client
            .get_feature(request)
            .await
            .map_err(|status| {
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
// Application state — owns the GraphQL schema + JWT secret
// ---------------------------------------------------------------------------

type AppSchema = Schema<Query, EmptyMutation, EmptySubscription>;

/// Shared application state injected into every axum handler via `State<Arc<AppState>>`.
struct AppState {
    schema: AppSchema,
    /// HS256 secret bytes. In dev, read from `JWT_SECRET` env var or the test
    /// constant. In prod, injected from External Secrets Operator (Sprint 3+).
    jwt_secret: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Serves the GraphQL endpoint.
///
/// Validates the `Authorization: Bearer <jwt>` header; on success, injects
/// the validated [`Claims`] (including `tenant_id` as [`TenantId`]) into the
/// GraphQL request data so resolvers can use them.
///
/// Returns HTTP 401 if the header is absent, the token is malformed, the
/// signature is invalid, or the token is expired.
async fn graphql_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: GraphQLRequest,
) -> Result<GraphQLResponse, StatusCode> {
    // Extract "Authorization: Bearer <token>"
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Decode and verify the JWT.
    let claims: Claims =
        decode_jwt(token, &state.jwt_secret).map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Parse tenant_id claim into a UUID.
    let tenant_id = Uuid::parse_str(&claims.tenant_id).map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Inject both the opaque TenantId (used by resolvers) and the full Claims
    // (available to resolvers that need sub or scopes).
    let gql_req = req
        .into_inner()
        .data(TenantId(tenant_id))
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

/// Build the axum `Router`, wiring the GraphQL schema, JWT middleware, and
/// health endpoint.
///
/// # Parameters
/// - `feature_store_grpc_url` — gRPC endpoint for the feature-store service,
///   e.g. `http://127.0.0.1:4001`. Channel is created lazily.
/// - `jwt_secret` — raw bytes of the HS256 signing secret. In tests pass the
///   `TEST_JWT_SECRET` constant; in production read `JWT_SECRET` from env.
pub async fn build_app(feature_store_grpc_url: &str, jwt_secret: Vec<u8>) -> Result<Router> {
    let channel = Channel::from_shared(feature_store_grpc_url.to_string())
        .expect("invalid feature-store URL")
        .connect_lazy();

    let fs_client = FeatureStoreServiceClient::new(channel);

    let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
        .data(fs_client)
        .finish();

    let state = Arc::new(AppState { schema, jwt_secret });

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/graphql", post(graphql_handler))
        .with_state(state);

    Ok(app)
}
