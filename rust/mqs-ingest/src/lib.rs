//! mqs-ingest — Martin-Quinn judicial ideal-point importer.
//!
//! Reads a CSV in the public Martin-Quinn release format, groups rows by
//! justice, sorts each group by term, writes the per-term series and a
//! `latest_score` / `latest_term` snapshot to `judges.bio.mqs`.
//!
//! Mirrors Sprint 7's `dime-ingest` crate. The matcher (three-pass: exact
//! court + name → name-only → last-name + court) is re-used from
//! `dime-ingest::matcher` so the two crates stay in lockstep on judge-row
//! resolution. Name preprocessing is also re-used — both releases use
//! "Last, First" form, and we want one canonical normaliser.
//!
//! Hot-path optimisation: the gateway's `extract_features_from_text`
//! reads a single JSONB scalar (`bio.mqs.latest_score`); the per-term
//! `scores[]` array is for audit / future date-aware lookups.

pub mod aggregator;
pub mod parser;

pub use aggregator::{aggregate_by_justice, AggregatedJustice};
pub use parser::{parse_mqs_csv, MqsRow};

/// Release tag stored in `bio.mqs.release`. Bump when we ingest a newer
/// Martin-Quinn drop. The tag is fully opaque to the matcher; it's only
/// used in the UI + the compliance disclosure.
pub const DEFAULT_RELEASE_TAG: &str = "mqs-2023-v1";
