//! Postgres integration test for the ingest-fetcher upsert path.
//!
//! Gated `#[ignore]` because it needs a running Postgres with the
//! `case_documents` migration applied. Run with:
//!     DATABASE_URL=postgres://... cargo test -p ingest-fetcher --tests -- --ignored

use std::fs::File;
use std::path::PathBuf;

use ingest_fetcher::{db, parse_tarball};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.tar.gz")
}

#[tokio::test]
#[ignore]
async fn fixture_upsert_is_idempotent() {
    let dsn = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set; run with -- --ignored only against a real Postgres");
    let pool = sqlx::PgPool::connect(&dsn)
        .await
        .expect("connect to Postgres");

    // Seed: parse + upsert.
    let f = File::open(fixture_path()).expect("open fixture");
    let opinions: Vec<_> = parse_tarball(f).into_iter().filter_map(|r| r.ok()).collect();
    assert_eq!(opinions.len(), 7, "seven valid opinions in fixture");

    db::upsert_opinions(&pool, opinions.clone())
        .await
        .expect("first upsert");

    let count_after_first: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM case_documents WHERE opinion_id BETWEEN 1001 AND 1007",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert!(count_after_first.0 >= 7, "seven opinions inserted");

    // Re-run: upsert the same data; count must not increase.
    db::upsert_opinions(&pool, opinions)
        .await
        .expect("second upsert");
    let count_after_second: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM case_documents WHERE opinion_id BETWEEN 1001 AND 1007",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(
        count_after_first.0, count_after_second.0,
        "ON CONFLICT DO UPDATE leaves count unchanged"
    );

    // Cleanup so subsequent runs start clean.
    sqlx::query("DELETE FROM case_documents WHERE opinion_id BETWEEN 1001 AND 1007")
        .execute(&pool)
        .await
        .expect("cleanup");
}
