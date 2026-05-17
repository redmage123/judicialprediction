//! jcs-ingest — Judicial Common Space (JCS) importer.
//!
//! Reads the Epstein/Martin/Quinn/Segal joint-scaling release, normalises
//! judge names, matches against the existing `judges` table via the shared
//! `dime_ingest::matcher`, patches `judges.bio.jcs` with a per-judge
//! ideology scalar + provenance.
//!
//! Shape mirrors `dime-ingest` more than `mqs-ingest`: a single static
//! value per judge, not a time series.  Sprint 10+ can switch to per-term
//! storage if the methodology becomes useful in time-varying form.

pub mod parser;

pub use parser::{parse_jcs_csv, JcsRow};

/// Release tag stored in `bio.jcs.release`. Bump when we ingest a newer
/// Epstein drop.
pub const DEFAULT_RELEASE_TAG: &str = "jcs-2018-v1";

/// Methodology / scaling vintage. Different from release because the JCS
/// project sometimes re-publishes the same scale on a newer file.
pub const DEFAULT_SCALE_TAG: &str = "epstein-2018";
