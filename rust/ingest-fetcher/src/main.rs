//! ingest-fetcher CLI.
//!
//! Subcommands:
//!   fetch <court>  — download a bulk dump to /tmp/jp-ingest-<court>.tar.gz
//!   parse <path>   — parse a tarball and report stats (no DB writes)
//!   run   <court>  — fetch + parse + upsert (the typical ingest path)

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ingest_fetcher::{db, fetch, parse_tarball};

#[derive(Parser)]
#[command(name = "ingest-fetcher", about = "CourtListener bulk-dump ingester")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download a court's bulk dump to /tmp/.
    Fetch { court: String },
    /// Parse a local tarball and print row counts (no DB writes).
    Parse { path: PathBuf },
    /// Fetch + parse + upsert into case_documents.
    Run {
        court: String,
        /// Postgres DSN. Defaults to $DATABASE_URL.
        #[arg(long)]
        database_url: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.cmd {
        Command::Fetch { court } => {
            let path = fetch::download_dump(&court).await?;
            println!("downloaded: {}", path.display());
        }
        Command::Parse { path } => {
            let file = std::fs::File::open(&path)
                .with_context(|| format!("open {}", path.display()))?;
            let mut ok = 0u64;
            let mut err = 0u64;
            for result in parse_tarball(file) {
                match result {
                    Ok(_) => ok += 1,
                    Err(e) => {
                        tracing::warn!(error = %e, "skipped malformed entry");
                        err += 1;
                    }
                }
            }
            println!("parsed: {ok} valid, {err} skipped");
        }
        Command::Run { court, database_url } => {
            let dsn = database_url
                .or_else(|| std::env::var("DATABASE_URL").ok())
                .context("provide --database-url or set DATABASE_URL")?;
            let pool = sqlx::PgPool::connect(&dsn)
                .await
                .context("connect to Postgres")?;
            let path = fetch::download_dump(&court).await?;
            let file = std::fs::File::open(&path)
                .with_context(|| format!("open {}", path.display()))?;
            let mut opinions = Vec::new();
            let mut skipped = 0u64;
            for result in parse_tarball(file) {
                match result {
                    Ok(op) => opinions.push(op),
                    Err(e) => {
                        tracing::warn!(error = %e, "skipped malformed entry");
                        skipped += 1;
                    }
                }
            }
            let stats = db::upsert_opinions(&pool, opinions).await?;
            println!(
                "ingested: {} touched, {skipped} skipped",
                stats.inserted + stats.skipped
            );
        }
    }
    Ok(())
}
