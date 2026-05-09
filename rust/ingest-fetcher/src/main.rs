//! ingest-fetcher CLI.
//!
//! Subcommands:
//!   fetch     <court>  — download a bulk dump to /tmp/jp-ingest-<court>.tar.gz
//!   parse     <path>   — parse a tarball and print row counts (no DB writes)
//!   run       <court>  — fetch + parse + upsert (bulk-data path; broken
//!                        when the egress IP is in CourtListener's WAF list)
//!   run-rest  <court>  — REST-API + upsert (the supported path; needs
//!                        $COURTLISTENER_TOKEN)

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ingest_fetcher::{db, fetch, parse_tarball, rest};

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
    /// Fetch + parse + upsert into case_documents (bulk-data path).
    Run {
        court: String,
        /// Postgres DSN. Defaults to $DATABASE_URL.
        #[arg(long)]
        database_url: Option<String>,
    },
    /// REST-API ingest + upsert into case_documents.
    /// Requires $COURTLISTENER_TOKEN. Use this when the bulk-data path is
    /// WAF-blocked (which is the common case from datacenter egress IPs).
    RunRest {
        court: String,
        /// Stop after this many opinions. Default 1000 to satisfy the S3.6
        /// acceptance bar; raise via --target if you want a fuller pull.
        #[arg(long, default_value_t = 1000)]
        target: usize,
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
        Command::RunRest { court, target, database_url } => {
            let dsn = database_url
                .or_else(|| std::env::var("DATABASE_URL").ok())
                .context("provide --database-url or set DATABASE_URL")?;
            let token = std::env::var("COURTLISTENER_TOKEN")
                .context("$COURTLISTENER_TOKEN must be set; get one from https://www.courtlistener.com/profile/api/")?;
            let pool = sqlx::PgPool::connect(&dsn)
                .await
                .context("connect to Postgres")?;
            let opinions = rest::fetch_opinions_via_rest(&token, &court, target).await?;
            let count = opinions.len();
            let stats = db::upsert_opinions(&pool, opinions).await?;
            println!(
                "REST ingested: {count} opinions for court='{court}' \
                 (touched={}, skipped={})",
                stats.inserted, stats.skipped
            );
        }
    }
    Ok(())
}
