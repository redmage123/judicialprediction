// JudicialPredict API Gateway — binary entry point.
//
// Reads from the environment:
//   FEATURE_STORE_GRPC_URL               — gRPC URL of feature-store (default http://127.0.0.1:4001)
//   ML_INFERENCE_GRPC_URL                — gRPC URL of ml-inference-svc (default http://127.0.0.1:51051)
//                                          ML_INFERENCE_URL accepted as a deprecated fallback.
//   DATABASE_URL                         — Postgres URL for the cases store (optional; when unset,
//                                          createCase/repredictCase return a "not configured" error)
//   GATEWAY_BIND                         — host:port for the HTTP/GraphQL server (default 0.0.0.0:4000)
//   JWT_SECRET                           — HS256 signing secret (required; no default)
//   RATE_LIMIT_RPM                       — max requests/min per tenant (default 60)
//   RATE_LIMIT_GRAPHQL_MUTATIONS_PER_MIN — max mutations/min per tenant (default 10)

use std::sync::Arc;

use anyhow::Result;
use api_gateway::RateLimitConfig;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let feature_store_url = std::env::var("FEATURE_STORE_GRPC_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:4001".to_string());

    let ml_inference_url = std::env::var("ML_INFERENCE_GRPC_URL")
        .or_else(|_| std::env::var("ML_INFERENCE_URL"))
        .unwrap_or_else(|_| "http://127.0.0.1:51051".to_string());
    if std::env::var("ML_INFERENCE_URL").is_ok() && std::env::var("ML_INFERENCE_GRPC_URL").is_err() {
        tracing::warn!("ML_INFERENCE_URL is deprecated (S5.4) — set ML_INFERENCE_GRPC_URL instead");
    }

    let bind_addr = std::env::var("GATEWAY_BIND").unwrap_or_else(|_| "0.0.0.0:4000".to_string());

    let jwt_secret = std::env::var("JWT_SECRET")
        .expect("JWT_SECRET environment variable must be set")
        .into_bytes();

    let requests_per_min: u32 = std::env::var("RATE_LIMIT_RPM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let mutations_per_min: u32 = std::env::var("RATE_LIMIT_GRAPHQL_MUTATIONS_PER_MIN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let rate_config = RateLimitConfig { requests_per_min, mutations_per_min };

    // Optional cases-store pool. When DATABASE_URL is unset (or invalid), GraphQL
    // mutations that touch the cases table return a deterministic error rather
    // than crashing the gateway.
    let cases_pool: Option<Arc<sqlx::PgPool>> = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => match PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(&url)
            .await
        {
            Ok(pool) => {
                tracing::info!("cases store connected (DATABASE_URL set)");
                // Run schema migrations on startup so a fresh DB (dev or prod)
                // comes up with the cases / predictions / case_documents / kg
                // tables already present.  Migrations live in the feature-store
                // crate which owns the schema; sqlx::migrate! reads them at
                // compile time so the binary is self-contained.
                //
                // Idempotent: sqlx tracks applied migrations in _sqlx_migrations
                // and skips anything already at the target version.
                if let Err(e) = sqlx::migrate!("../feature-store/migrations")
                    .run(&pool)
                    .await
                {
                    tracing::error!("schema migration failed: {e}");
                    return Err(e.into());
                }
                tracing::info!("schema migrations applied");
                Some(Arc::new(pool))
            }
            Err(e) => {
                tracing::warn!("DATABASE_URL set but pool failed: {e} — mutations will return 'not configured'");
                None
            }
        },
        _ => {
            tracing::warn!("DATABASE_URL unset — cases mutations will return 'not configured'");
            None
        }
    };

    tracing::info!(
        "api-gateway: feature-store gRPC at {feature_store_url}, \
         ml-inference gRPC at {ml_inference_url}, \
         rate-limit {requests_per_min} rpm / {mutations_per_min} mutations/min"
    );
    // Audit recorder gets its own connection to AUDIT_DATABASE_URL (falls back
    // to DATABASE_URL).  When neither is set, audit writes are silently skipped
    // so the gateway still runs in dev contexts without a database.
    let audit_recorder: Option<audit_recorder::AuditRecorder> =
        match std::env::var("AUDIT_DATABASE_URL")
            .ok()
            .or_else(|| std::env::var("DATABASE_URL").ok())
        {
            Some(url) if !url.is_empty() => match PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(std::time::Duration::from_secs(5))
                .connect(&url)
                .await
            {
                Ok(pool) => Some(audit_recorder::AuditRecorder::new(pool)),
                Err(e) => {
                    tracing::warn!("audit pool connect failed: {e} — audit writes disabled");
                    None
                }
            },
            _ => {
                tracing::warn!("AUDIT_DATABASE_URL unset — audit writes disabled");
                None
            }
        };

    let app = api_gateway::build_app(
        &feature_store_url,
        &ml_inference_url,
        cases_pool,
        audit_recorder,
        jwt_secret,
        rate_config,
    )
    .await?;

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("api-gateway listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
