// JudicialPredict API Gateway — application core
//
// All GraphQL types, handlers, and the `build_app` factory live here.
// `src/lib.rs` re-exports `build_app`; `src/main.rs` just binds the
// TCP listener and calls into the library.
//
// SECURITY: every request must supply an `X-Tenant-Id: <uuid>` header.
// The gateway injects it into the GraphQL request data; resolvers call
// `feature_store::set_tenant_context` before every DB query, ensuring
// Postgres RLS evaluates the correct tenant isolation policy.

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
use sqlx::PgPool;
use std::sync::Arc;
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
    /// Storage primary key (UUID).
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

impl From<feature_store::FeatureRow> for FeatureDto {
    fn from(r: feature_store::FeatureRow) -> Self {
        Self {
            id: r.id.to_string(),
            name: r.name,
            value_json: r.value.to_string(),
            tier: r.tier,
            sensitivity: r.sensitivity,
            case_id: r.case_id.map(|id| id.to_string()),
        }
    }
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
    /// different tenant (RLS enforces isolation transparently).
    ///
    /// Requires `X-Tenant-Id` header on the HTTP request.
    async fn feature(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> async_graphql::Result<Option<FeatureDto>> {
        let pool = ctx.data::<PgPool>().map_err(|_| "missing pool")?;
        let TenantId(tenant_id) = *ctx.data::<TenantId>().map_err(|_| "missing tenant id")?;

        let feature_id = Uuid::parse_str(&id)
            .map_err(|_| async_graphql::Error::new("id must be a valid UUID"))?;

        let mut tx = feature_store::set_tenant_context(pool, tenant_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let row = feature_store::get_feature(&mut tx, feature_id, tenant_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(row.map(FeatureDto::from))
    }
}

// ---------------------------------------------------------------------------
// GraphQL handler — bridges axum headers into the GraphQL request data
// ---------------------------------------------------------------------------

type AppSchema = Schema<Query, EmptyMutation, EmptySubscription>;

/// Serves the GraphQL endpoint.
///
/// Extracts `X-Tenant-Id` from HTTP headers and injects it into the GraphQL
/// request's data map so resolvers can call `set_tenant_context`.
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
/// `database_url` must point at the Postgres instance as the `jp_app` role.
pub async fn build_app(database_url: &str) -> Result<Router> {
    let pool = PgPool::connect(database_url).await?;

    let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
        .data(pool)
        .finish();

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/graphql", post(graphql_handler))
        .with_state(Arc::new(schema));

    Ok(app)
}
