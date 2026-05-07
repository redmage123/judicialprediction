// JudicialPredict API Gateway — binary entry point.
//
// Reads FEATURE_STORE_GRPC_URL from the environment (default: http://127.0.0.1:4001)
// and binds the HTTP/GraphQL server on 0.0.0.0:4000.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let feature_store_url = std::env::var("FEATURE_STORE_GRPC_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:4001".to_string());

    tracing::info!("api-gateway: feature-store gRPC at {feature_store_url}");
    let app = api_gateway::build_app(&feature_store_url).await?;

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4000").await?;
    tracing::info!("api-gateway listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
