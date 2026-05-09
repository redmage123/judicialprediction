//! Imperative shell: download a CourtListener bulk dump to disk.
//!
//! Sprint-2 scope is fixture-only — this module is not exercised by tests.
//! It exists so the `run <court>` CLI path is wired up; a Sprint-3 story
//! adds a real-network smoke test and switches to chunked streaming for
//! large dumps. For Sprint-2 we keep the implementation small and load
//! the whole response into memory (acceptable for the 30 MB tax dump).

use std::path::PathBuf;

use anyhow::{Context, Result};

const COURTLISTENER_BULK_BASE: &str = "https://www.courtlistener.com/api/bulk-data/opinions";

/// Download the bulk dump for a single court to `/tmp/jp-ingest-<court>.tar.gz`.
pub async fn download_dump(court: &str) -> Result<PathBuf> {
    let url = format!("{COURTLISTENER_BULK_BASE}/{court}.tar.gz");
    let dest = PathBuf::from(format!("/tmp/jp-ingest-{court}.tar.gz"));

    tracing::info!(url = %url, dest = %dest.display(), "downloading bulk dump");

    let resp = reqwest::get(&url)
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx from {url}"))?;

    let bytes = resp.bytes().await.context("read response body")?;
    tokio::fs::write(&dest, &bytes)
        .await
        .with_context(|| format!("write {}", dest.display()))?;

    tracing::info!(bytes = bytes.len(), "download complete");
    Ok(dest)
}
