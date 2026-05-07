// JudicialPredict feature-store — standalone gRPC server binary.
//
// Reads DATABASE_URL from the environment (defaults to the dev stack DSN).
// Binds the FeatureStoreService on 0.0.0.0:4001.
//
// Migrations are NOT run here — apply them with sqlx-cli before starting
// (or let the test harness apply them). The jp_app role used by this binary
// does not have sufficient privileges to modify the _sqlx_migrations table.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://jp_app:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev".to_string()
    });

    tracing::info!("feature-store-server: connecting to Postgres");
    let pool = sqlx::PgPool::connect(&database_url).await?;

    let server = feature_store::server::FeatureStoreServer::new(pool);

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
