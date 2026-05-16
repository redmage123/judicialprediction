//! dime-ingest binary.
//!
//! Subcommands:
//!
//!   * `ingest --csv <path> --tenant-id <uuid> [--report <path>]
//!             [--release <tag>] [--dry-run]`
//!         Read a Bonica DIME judges CSV and write `bio.dime` patches into
//!         the `judges` table for `tenant_id`. Report file lists unmatched
//!         rows for manual review.
//!
//! The binary is deliberately scope-limited: no model retraining, no
//! migration application, no audit emission. The patches it writes are
//! purely advisory enrichment, consumed by the gateway's prefill path.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use dime_ingest::{
    matcher::match_row,
    parser::parse_dime_csv,
    DEFAULT_RELEASE_TAG,
};

#[derive(Parser, Debug)]
#[command(name = "dime-ingest", about = "Bonica DIME judge-ideology importer")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Ingest a Bonica DIME judges CSV and write bio.dime patches.
    Ingest {
        /// Path to a Bonica DIME judges CSV (headers required).
        #[arg(long)]
        csv: PathBuf,

        /// Tenant UUID to scope the import to.
        #[arg(long)]
        tenant_id: Uuid,

        /// Path to write unmatched rows for human review. Optional.
        #[arg(long)]
        report: Option<PathBuf>,

        /// Release tag to stamp in bio.dime.release.
        #[arg(long, default_value = DEFAULT_RELEASE_TAG)]
        release: String,

        /// Don't write to the DB — just report what would happen.
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Ingest { csv, tenant_id, report, release, dry_run } => {
            run_ingest(csv, tenant_id, report, release, dry_run).await
        }
    }
}

async fn run_ingest(
    csv: PathBuf,
    tenant_id: Uuid,
    report: Option<PathBuf>,
    release: String,
    dry_run: bool,
) -> Result<()> {
    let started = Instant::now();
    let rows = parse_dime_csv(&csv).with_context(|| format!("parse {csv:?}"))?;
    tracing::info!(rows = rows.len(), "parsed CSV");

    let db_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL env var must be set")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&db_url)
        .await?;

    let mut updated = 0u32;
    let mut unmatched = 0u32;
    let mut skipped_no_cfscore = 0u32;
    let mut unmatched_lines: Vec<String> = Vec::new();

    for row in &rows {
        let Some(cfscore) = row.cfscore else {
            skipped_no_cfscore += 1;
            continue;
        };

        match match_row(&pool, tenant_id, row).await? {
            None => {
                unmatched += 1;
                unmatched_lines.push(format!(
                    "{}\t{}\t{}\t{}",
                    row.bonica_id, row.name, row.court, cfscore
                ));
            }
            Some(m) => {
                if !dry_run {
                    let patch = serde_json::json!({
                        "dime": {
                            "cfscore": cfscore,
                            "release": release,
                            "source_id": row.bonica_id,
                            "ingested_at": Utc::now().to_rfc3339(),
                            "match_confidence": m.confidence.as_str(),
                        }
                    });
                    let res = sqlx::query(
                        r#"
                        UPDATE judges
                           SET bio = bio || $1::jsonb,
                               updated_at = now()
                         WHERE tenant_id = $2
                           AND id = $3
                        "#,
                    )
                    .bind(&patch)
                    .bind(tenant_id)
                    .bind(m.judge_id)
                    .execute(&pool)
                    .await?;
                    if res.rows_affected() > 0 {
                        updated += 1;
                    }
                }
                tracing::debug!(
                    judge_id = %m.judge_id,
                    confidence = ?m.confidence,
                    cfscore = cfscore,
                    "matched + patched"
                );
            }
        }
    }

    if let Some(path) = report {
        std::fs::write(
            &path,
            format!(
                "# bonica_id\tname\tcourt\tcfscore\n{}",
                unmatched_lines.join("\n")
            ),
        )
        .with_context(|| format!("write report {path:?}"))?;
        tracing::info!(path = %path.display(), count = unmatched_lines.len(), "report written");
    }

    let elapsed_ms = started.elapsed().as_millis();
    tracing::info!(
        rows_total = rows.len(),
        judges_updated = updated,
        rows_unmatched = unmatched,
        rows_skipped_no_cfscore = skipped_no_cfscore,
        dry_run,
        elapsed_ms,
        "ingest complete"
    );

    Ok(())
}
