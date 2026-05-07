// Functional-leaning (ADR-FP-001 Tier 2).
// Separate process from api-gateway; handles partner OAuth 2 flows + GraphQL.

use axum::{Router, routing::get};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new().route("/health", get(|| async { "ok" }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4001").await.unwrap();
    tracing::info!("partner-gateway listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
