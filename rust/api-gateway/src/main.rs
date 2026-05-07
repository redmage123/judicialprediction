// JudicialPredict API Gateway — binary entry point
//
// Binds the TCP listener and delegates everything to the library crate.
// The library (`src/lib.rs` → `src/app.rs`) owns all types and handlers
// so integration tests can instantiate the router without spawning a process.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://jp_app:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev".to_string()
    });

    tracing::info!("connecting to Postgres");
    let app = api_gateway::build_app(&database_url).await?;

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4000").await?;
    tracing::info!("api-gateway listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
