//! jcs-ingest binary. `ingest --csv <path> --tenant-id <uuid>
//! [--report <path>] [--release <tag>] [--scale <tag>] [--dry-run]`.

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
use jcs_ingest::{parser::parse_jcs_csv, DEFAULT_RELEASE_TAG, DEFAULT_SCALE_TAG};

#[derive(Parser, Debug)]
#[command(name = "jcs-ingest", about = "Judicial Common Space ideology importer")]
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
        #[arg(long, default_value = DEFAULT_SCALE_TAG)]
        scale: String,
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Ingest { csv, tenant_id, report, release, scale, dry_run } => {
            run_ingest(csv, tenant_id, report, release, scale, dry_run).await
        }
    }
}

async fn run_ingest(
    csv: PathBuf,
    tenant_id: Uuid,
    report: Option<PathBuf>,
    release: String,
    scale: String,
    dry_run: bool,
) -> Result<()> {
    let started = Instant::now();
    let rows = parse_jcs_csv(&csv).with_context(|| format!("parse {csv:?}"))?;
    tracing::info!(rows = rows.len(), "parsed CSV");

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&db_url)
        .await?;

    let mut updated = 0u32;
    let mut unmatched = 0u32;
    let mut skipped_no_score = 0u32;
    let mut unmatched_lines: Vec<String> = Vec::new();

    for row in &rows {
        let Some(score) = row.jcs else {
            skipped_no_score += 1;
            continue;
        };

        // Re-use dime-ingest's matcher via a DimeRow probe. The matcher
        // never reads cfscore, so passing the JCS score in that slot is
        // safe — it's a structural shape match, not a semantic one.
        let probe = DimeRow {
            bonica_id: row.judge_id.clone(),
            name: row.judge_name.clone(),
            court: row.court.clone(),
            cfscore: Some(score),
        };

        match match_row(&pool, tenant_id, &probe).await? {
            None => {
                unmatched += 1;
                unmatched_lines.push(format!(
                    "{}\t{}\t{}\t{}",
                    row.judge_id, row.judge_name, row.court, score
                ));
            }
            Some(m) => {
                if !dry_run {
                    let patch = json!({
                        "jcs": {
                            "score": score,
                            "scale": scale,
                            "release": release,
                            "source_id": row.judge_id,
                            "ingested_at": Utc::now().to_rfc3339(),
                            "match_confidence": m.confidence.as_str(),
                        }
                    });
                    let res = sqlx::query(
                        r#"UPDATE judges SET bio = bio || $1::jsonb, updated_at = now()
                              WHERE tenant_id = $2 AND id = $3"#,
                    )
                    .bind(&patch)
                    .bind(tenant_id)
                    .bind(m.judge_id)
                    .execute(&pool)
                    .await?;
                    if res.rows_affected() > 0 { updated += 1; }
                }
                tracing::debug!(judge_id = %m.judge_id, confidence = ?m.confidence,
                                score, "matched + patched");
            }
        }
    }

    if let Some(path) = report {
        std::fs::write(
            &path,
            format!("# judge_id\tname\tcourt\tjcs\n{}", unmatched_lines.join("\n")),
        )
        .with_context(|| format!("write report {path:?}"))?;
        tracing::info!(path = %path.display(), count = unmatched_lines.len(), "report written");
    }

    let elapsed_ms = started.elapsed().as_millis();
    tracing::info!(
        rows_total = rows.len(),
        judges_updated = updated,
        rows_unmatched = unmatched,
        rows_skipped_no_score = skipped_no_score,
        dry_run,
        elapsed_ms,
        "ingest complete"
    );

    Ok(())
}
