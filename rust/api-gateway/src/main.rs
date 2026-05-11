// JudicialPredict API Gateway — binary entry point.
//
// Reads from the environment:
//   FEATURE_STORE_GRPC_URL               — gRPC address of the feature-store service
//                                          (default: http://127.0.0.1:4001)
//   JWT_SECRET                           — HS256 signing secret (required; no default)
//   RATE_LIMIT_RPM                       — max requests/min per tenant (default 60)
//   RATE_LIMIT_GRAPHQL_MUTATIONS_PER_MIN — max mutations/min per tenant (default 10)
//
// Binds the HTTP/GraphQL server on 0.0.0.0:4000.

use anyhow::Result;
use api_gateway::RateLimitConfig;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let feature_store_url = std::env::var("FEATURE_STORE_GRPC_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:4001".to_string());

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

    tracing::info!(
        "api-gateway: feature-store gRPC at {feature_store_url}, \
         rate-limit {requests_per_min} rpm / {mutations_per_min} mutations/min"
    );
    let app = api_gateway::build_app(&feature_store_url, jwt_secret, rate_config).await?;

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4000").await?;
    tracing::info!("api-gateway listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
