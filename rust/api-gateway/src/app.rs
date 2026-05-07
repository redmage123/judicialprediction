// JudicialPredict API Gateway — application core.
//
// All GraphQL types, handlers, and the `build_app` factory live here.
// `src/lib.rs` re-exports `build_app`; `src/main.rs` just binds the
// TCP listener and calls into the library.
//
// SECURITY: every request must supply an `X-Tenant-Id: <uuid>` header.
// The gateway injects it into the GraphQL request data; resolvers attach it
// as gRPC metadata on every call to the feature-store-server so Postgres RLS
// evaluates the correct tenant isolation policy.

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

// ---------------------------------------------------------------------------
// Tenant identity — injected per request into the GraphQL context
// ---------------------------------------------------------------------------

/// Wraps the tenant UUID extracted from the `X-Tenant-Id` HTTP header.
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
    /// Requires `X-Tenant-Id` header on the HTTP request.
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
// GraphQL handler — bridges axum headers into the GraphQL request data
// ---------------------------------------------------------------------------

type AppSchema = Schema<Query, EmptyMutation, EmptySubscription>;

/// Serves the GraphQL endpoint.
///
/// Extracts `X-Tenant-Id` from HTTP headers and injects it into the GraphQL
/// request's data map so resolvers can attach it as gRPC metadata.
/// Returns HTTP 401 if the header is absent or not a valid UUID.
async fn graphql_handler(
    State(schema): State<Arc<AppSchema>>,
    headers: HeaderMap,
    req: GraphQLRequest,
) -> Result<GraphQLResponse, StatusCode> {
    let tenant_str = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let tenant_id = Uuid::parse_str(tenant_str).map_err(|_| StatusCode::UNAUTHORIZED)?;

    let gql_req = req.into_inner().data(TenantId(tenant_id));
    Ok(schema.execute(gql_req).await.into())
}

/// Simple HTTP liveness probe — does not require authentication.
async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

// ---------------------------------------------------------------------------
// App builder — exported via lib.rs for use in tests
// ---------------------------------------------------------------------------

/// Build the axum `Router`, wiring the GraphQL schema and health endpoint.
///
/// `feature_store_grpc_url` is the gRPC endpoint for the feature-store service,
/// e.g. `http://127.0.0.1:4001`. The channel is created lazily so the service
/// does not need to be up at startup time.
pub async fn build_app(feature_store_grpc_url: &str) -> Result<Router> {
    let channel = Channel::from_shared(feature_store_grpc_url.to_string())
        .expect("invalid feature-store URL")
        .connect_lazy();

    let fs_client = FeatureStoreServiceClient::new(channel);

    let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
        .data(fs_client)
        .finish();

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/graphql", post(graphql_handler))
        .with_state(Arc::new(schema));

    Ok(app)
}
