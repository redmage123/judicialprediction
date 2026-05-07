use axum::{Router, routing::get};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new().route("/health", get(|| async { "ok" }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4000").await.unwrap();
    tracing::info!("api-gateway listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
