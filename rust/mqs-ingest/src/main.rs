//! mqs-ingest binary.
//!
//! `ingest --csv <path> --tenant-id <uuid> [--report <path>]
//!         [--release <tag>] [--dry-run]`
//!
//! Reads a Martin-Quinn CSV, aggregates by justiceID, matches each
//! justice against our `judges` table using the dime-ingest matcher,
//! patches `judges.bio.mqs` with the full per-term series + a
//! `latest_score`/`latest_term` snapshot.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use dime_ingest::matcher::match_row;
use dime_ingest::parser::DimeRow;
use mqs_ingest::{aggregator::aggregate_by_justice, parser::parse_mqs_csv, DEFAULT_RELEASE_TAG};

#[derive(Parser, Debug)]
#[command(name = "mqs-ingest", about = "Martin-Quinn judicial ideal-point importer")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    Ingest {
        #[arg(long)]
        csv: PathBuf,
        #[arg(long)]
        tenant_id: Uuid,
        #[arg(long)]
        report: Option<PathBuf>,
        #[arg(long, default_value = DEFAULT_RELEASE_TAG)]
        release: String,
        #[arg(long)]
        dry_run: bool,
        /// CourtListener court slug for SCOTUS justices. Used by the
        /// matcher's name+court pass since MQ doesn't carry a court
        /// column itself (every row is SCOTUS by definition).
        #[arg(long, default_value = "scotus")]
        court: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Ingest {
            csv,
            tenant_id,
            report,
            release,
            dry_run,
            court,
        } => run_ingest(csv, tenant_id, report, release, dry_run, court).await,
    }
}

async fn run_ingest(
    csv: PathBuf,
    tenant_id: Uuid,
    report: Option<PathBuf>,
    release: String,
    dry_run: bool,
    court: String,
) -> Result<()> {
    let started = Instant::now();
    let rows = parse_mqs_csv(&csv).with_context(|| format!("parse {csv:?}"))?;
    tracing::info!(rows = rows.len(), "parsed CSV");

    let aggregated = aggregate_by_justice(rows);
    tracing::info!(justices = aggregated.len(), "aggregated");

    let db_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL env var must be set")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&db_url)
        .await?;

    let mut updated = 0u32;
    let mut unmatched = 0u32;
    let mut skipped_no_latest = 0u32;
    let mut unmatched_lines: Vec<String> = Vec::new();

    for j in &aggregated {
        let Some(latest_score) = j.latest_score else {
            skipped_no_latest += 1;
            continue;
        };
        let Some(latest_term) = j.latest_term else {
            skipped_no_latest += 1;
            continue;
        };

        // Re-use dime-ingest's matcher by faking a DimeRow.  MQ rows are
        // all SCOTUS so we pass `court` explicitly from the CLI flag.
        let probe = DimeRow {
            bonica_id: j.justice_id.clone(),
            name: j.name.clone(),
            court: court.clone(),
            cfscore: Some(latest_score),
        };

        match match_row(&pool, tenant_id, &probe).await? {
            None => {
                unmatched += 1;
                unmatched_lines.push(format!(
                    "{}\t{}\t{}\t{}\t{}",
                    j.justice_id, j.name, court, latest_term, latest_score
                ));
            }
            Some(m) => {
                if !dry_run {
                    let scores_json: Vec<serde_json::Value> = j
                        .scores
                        .iter()
                        .map(|s| {
                            json!({
                                "term": s.term,
                                "post_mean": s.post_mean,
                                "post_sd": s.post_sd,
                            })
                        })
                        .collect();
                    let patch = json!({
                        "mqs": {
                            "scores": scores_json,
                            "latest_score": latest_score,
                            "latest_term": latest_term,
                            "release": release,
                            "source_id": j.justice_id,
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
                    latest_term, latest_score,
                    terms = j.scores.len(),
                    "matched + patched"
                );
            }
        }
    }

    if let Some(path) = report {
        std::fs::write(
            &path,
            format!(
                "# justice_id\tname\tcourt\tlatest_term\tlatest_score\n{}",
                unmatched_lines.join("\n")
            ),
        )
        .with_context(|| format!("write report {path:?}"))?;
        tracing::info!(path = %path.display(), count = unmatched_lines.len(), "report written");
    }

    let elapsed_ms = started.elapsed().as_millis();
    tracing::info!(
        rows_total = aggregated.len(),
        judges_updated = updated,
        justices_unmatched = unmatched,
        justices_skipped_no_latest = skipped_no_latest,
        dry_run,
        elapsed_ms,
        "ingest complete"
    );

    Ok(())
}
