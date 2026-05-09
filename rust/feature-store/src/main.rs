// JudicialPredict feature-store — standalone gRPC server binary.
//
// Reads DATABASE_URL from the environment (defaults to the dev stack DSN).
// Binds the FeatureStoreService on 0.0.0.0:4001 (gRPC).
//
// S2.12: also starts a lightweight admin HTTP server on 0.0.0.0:4002 that
// exposes POST /admin/tenant-settings for updating per-tenant override config.
// Sprint-3 follow-up: replace the static ADMIN_TOKEN bearer check with proper
// JWT validation (JP-38 / ADR-003 operator RBAC).
//
// Migrations are NOT run here — apply them with sqlx-cli before starting.
// The jp_app role does not have sufficient privileges to modify _sqlx_migrations.

use anyhow::Result;
use audit_recorder::AuditRecorder;
use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use uuid::Uuid;

use feature_store::tenant_settings::{self, OverridesCache, TenantOverrides};

// ---------------------------------------------------------------------------
// Admin HTTP state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AdminState {
    pool: sqlx::PgPool,
    cache: OverridesCache,
    recorder: AuditRecorder,
    /// Static bearer token — Sprint-3: replace with proper JWT validation (JP-38).
    admin_token: String,
}

#[derive(Deserialize)]
struct UpdateSettingsRequest {
    tenant_id: Uuid,
    overrides: TenantOverrides,
}

/// POST /admin/tenant-settings
///
/// Requires `Authorization: Bearer <ADMIN_TOKEN>` header.
/// Updates the per-tenant feature-tier override config and emits audit events
/// for each changed key via the audit-recorder.
async fn admin_update_settings(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(body): Json<UpdateSettingsRequest>,
) -> impl IntoResponse {
    // Static bearer-token guard — Sprint-3: replace with JWT validation.
    let authorized = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|token| token == state.admin_token)
        .unwrap_or(false);

    if !authorized {
        return (StatusCode::UNAUTHORIZED, "invalid or missing admin token").into_response();
    }

    match tenant_settings::update_overrides(
        &state.pool,
        body.tenant_id,
        body.overrides,
        &state.cache,
        &state.recorder,
    )
    .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://jp_app:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev".to_string()
    });
    // Static admin token; Sprint-3 will replace this with JWT (JP-38).
    let admin_token = std::env::var("ADMIN_TOKEN")
        .unwrap_or_else(|_| "dev-admin-token-change-in-production".to_string());

    tracing::info!("feature-store-server: connecting to Postgres");
    let pool = sqlx::PgPool::connect(&database_url).await?;

    // S2.12: shared override cache and audit recorder (shared with gRPC handlers).
    let cache = OverridesCache::new();
    let recorder = AuditRecorder::new(pool.clone());

    // ── Admin HTTP server (port 4002) ────────────────────────────────────────
    let admin_state = AdminState {
        pool: pool.clone(),
        cache: cache.clone(),
        recorder: recorder.clone(),
        admin_token,
    };
    let admin_app = Router::new()
        .route("/admin/tenant-settings", post(admin_update_settings))
        .with_state(admin_state);

    let admin_addr: std::net::SocketAddr = "0.0.0.0:4002".parse()?;
    let admin_listener = tokio::net::TcpListener::bind(admin_addr).await?;
    tracing::info!("feature-store admin HTTP listening on {admin_addr}");
    tokio::spawn(async move {
        axum::serve(admin_listener, admin_app)
            .await
            .expect("admin HTTP server error");
    });

    // ── gRPC server (port 4001) ──────────────────────────────────────────────
    let server = feature_store::server::FeatureStoreServer::new(pool, cache, recorder);

    let addr: std::net::SocketAddr = "0.0.0.0:4001".parse()?;
    tracing::info!("feature-store-server listening on {addr}");

    tonic::transport::Server::builder()
        .add_service(
            feature_store::judicialpredict::data_plane::feature_store::v1::feature_store_service_server::FeatureStoreServiceServer::new(server),
        )
        .serve(addr)
        .await?;

    Ok(())
}
