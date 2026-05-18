//! ingest-fetcher — CourtListener bulk-dump ingester.
//!
//! Architecture: ADR-FP-001 functional-core / imperative-shell.
//! - `parse` is pure: tarball bytes → `Iterator<Result<Opinion>>`. No I/O, no DB.
//! - `fetch` and `db` are the imperative shell.
//!
//! Sprint-2 scope is fixture-only. Real-network smoke is Sprint-3.

pub mod cap;
pub mod citations;
pub mod db;
pub mod extract;
pub mod fetch;
pub mod kg;
pub mod parse;
pub mod rest;

pub use citations::{
    extract_uri_strings, parse_opinion_id_from_uri, populate_citations, CitationStats,
};
pub use extract::{classify_case_type, detect_outcome, run_extraction, ExtractStats};
pub use kg::{
    extract_attorney_names, extract_judge_names, normalize_attorney_name, normalize_judge_name,
    populate_from_case_documents, AttorneySide, PopulateStats,
};
pub use parse::{parse_tarball, Opinion};
