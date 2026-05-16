//! dime-ingest — Bonica DIME judge-ideology importer.
//!
//! Reads a CSV in the Bonica DIME judge format, normalises judge names to the
//! same shape that `ingest-fetcher` writes into `judges.normalized_name`,
//! looks each row up in the existing `judges` table, and patches
//! `judges.bio.dime` with the campaign-finance ideology score (`cfscore`) and
//! provenance.
//!
//! Designed for batch backfill from a local copy of the public release. No
//! live HTTP — DIME is a tarball drop, not an API.
//!
//! The matcher is deliberately conservative: we only write when we're sure
//! the row corresponds to a judge already in our KG. Unmatched rows are
//! emitted to the `--report` file for human review rather than guessed at.

pub mod matcher;
pub mod parser;

pub use matcher::{match_row, MatchConfidence, MatchResult};
pub use parser::{parse_dime_csv, DimeRow};

/// Release tag stored in `bio.dime.release`. Bump when we ingest a newer
/// Bonica drop so predictions can be reproduced against the same vintage.
pub const DEFAULT_RELEASE_TAG: &str = "dime-2014-judges-v1.0";
