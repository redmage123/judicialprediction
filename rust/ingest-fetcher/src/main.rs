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
use ingest_fetcher::{db, extract, fetch, kg, parse_tarball, rest};
use uuid::Uuid;

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
    /// S5.6: scan `case_documents` and populate KG nodes (`courts`, `judges`)
    /// for a given tenant.  Idempotent; safe to re-run.
    PopulateKg {
        /// UUID of the tenant to write nodes against.  Defaults to the
        /// dev-tenant when --tenant-id is omitted (handy for the local
        /// docker-compose stack).
        #[arg(long, default_value = "00000000-0000-0000-0000-000000000001")]
        tenant_id: Uuid,
        /// Postgres DSN. Defaults to $DATABASE_URL.
        #[arg(long)]
        database_url: Option<String>,
    },
    /// S5.7: scan `case_documents`, classify `case_type` + `outcome_for`, and
    /// roll up per-judge severity into `judges.bio`.  Idempotent — only acts
    /// on rows with `features_extracted_at IS NULL`, so a forced re-run needs
    /// `UPDATE case_documents SET features_extracted_at = NULL` first.
    ExtractFeatures {
        #[arg(long, default_value = "00000000-0000-0000-0000-000000000001")]
        tenant_id: Uuid,
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
            let stats = rest::fetch_and_upsert_via_rest(&pool, &token, &court, target).await?;
            println!(
                "REST ingest for court='{court}' — stored={} target={} \
                 daily_cap_hit={} already_in_db_skipped={} \
                 empty_text_skipped={} http_errors={}",
                stats.stored,
                target,
                stats.daily_cap_hit,
                stats.already_in_db_skipped,
                stats.empty_text_skipped,
                stats.http_errors,
            );
            // Exit cleanly if we just hit the daily cap — caller (cron) gets exit 0.
        }
        Command::PopulateKg { tenant_id, database_url } => {
            let dsn = database_url
                .or_else(|| std::env::var("DATABASE_URL").ok())
                .context("provide --database-url or set DATABASE_URL")?;
            let pool = sqlx::PgPool::connect(&dsn)
                .await
                .context("connect to Postgres")?;
            let stats = kg::populate_from_case_documents(&pool, tenant_id).await?;
            println!(
                "populate-kg tenant={tenant_id} \
                 docs_scanned={} courts_inserted={} courts_existing={} \
                 judges_inserted={} judges_existing={}",
                stats.case_documents_scanned,
                stats.courts_inserted,
                stats.courts_existing,
                stats.judges_inserted,
                stats.judges_existing,
            );
        }
        Command::ExtractFeatures { tenant_id, database_url } => {
            let dsn = database_url
                .or_else(|| std::env::var("DATABASE_URL").ok())
                .context("provide --database-url or set DATABASE_URL")?;
            let pool = sqlx::PgPool::connect(&dsn)
                .await
                .context("connect to Postgres")?;
            let stats = extract::run_extraction(&pool, tenant_id).await?;
            println!(
                "extract-features tenant={tenant_id} \
                 docs_scanned={} case_type_set={} outcome_set={} judges_updated={}",
                stats.docs_scanned,
                stats.case_type_set,
                stats.outcome_set,
                stats.judges_updated,
            );
        }
    }
    Ok(())
}
