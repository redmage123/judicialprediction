//! ingest-fetcher — CourtListener bulk-dump ingester.
//!
//! Architecture: ADR-FP-001 functional-core / imperative-shell.
//! - `parse` is pure: tarball bytes → `Iterator<Result<Opinion>>`. No I/O, no DB.
//! - `fetch` and `db` are the imperative shell.
//!
//! Sprint-2 scope is fixture-only. Real-network smoke is Sprint-3.

pub mod db;
pub mod fetch;
pub mod parse;

pub use parse::{parse_tarball, Opinion};
