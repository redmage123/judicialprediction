//! Knowledge-graph populator (S5.6).
//!
//! Walks `case_documents` and writes nodes to the S5.5 KG tables (`courts`,
//! `judges`).  Idempotent: re-runs upsert via `ON CONFLICT DO NOTHING` so the
//! cron / dev-loop replay stays clean.
//!
//! # Out of scope (deferred to later sprints)
//!
//! - **`case_courts` / `case_judges` / `case_citations` edges.**  These FK
//!   to `cases(id)`, but the public CourtListener corpus lives in a separate
//!   table (`case_documents`); we have no operator-created `cases` rows to
//!   attach the edges to.  Future work: either import case_documents into
//!   `cases` with a `case_origin = 'public_corpus'` discriminator, or extend
//!   the edge tables to accept either source.
//!
//! - **Citations from the CourtListener `cites` array.**  The `case_documents`
//!   schema (S2.17) does not store the `cites` array; the ingest fetcher
//!   would need a column add + a re-pull.  Tracked as Sprint 6 follow-up.
//!
//! # What it does ship
//!
//! 1. Court nodes — one row per `DISTINCT case_documents.court_id`, keyed on
//!    `(tenant_id, source='courtlistener', source_id=court_id)`.
//! 2. Judge nodes — regex-extracted from the opinion header (first ~5 KB of
//!    `full_text_plain`), unique on `(tenant_id, normalized_name)`, linked
//!    to the court they were first seen in via `primary_court_id`.

use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Per-run stats returned by [`populate_from_case_documents`].
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PopulateStats {
    pub case_documents_scanned: u64,
    pub courts_inserted: u64,
    pub courts_existing: u64,
    pub judges_inserted: u64,
    pub judges_existing: u64,
}

/// Walk `case_documents` and populate `courts` + `judges` for `tenant_id`.
///
/// Connection must run with sufficient privileges to write the KG tables
/// (either the migration role or `jp_app` with `SET LOCAL app.current_tenant_id`).
/// This function does the `SET LOCAL` itself so RLS sees the tenant.
pub async fn populate_from_case_documents(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<PopulateStats> {
    let mut tx = pool.begin().await.context("begin populate-kg tx")?;

    // RLS reads `app.current_tenant_id`; set it for this transaction so the
    // KG inserts pass the `tenant_isolation` USING/WITH CHECK clauses.
    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{tenant_id}'"
    ))
    .execute(&mut *tx)
    .await
    .context("SET LOCAL app.current_tenant_id")?;

    let mut stats = PopulateStats::default();

    // ── 1. Courts ──────────────────────────────────────────────────────────
    // Read distinct CL court slugs and the jurisdiction they map to.  We only
    // store one jurisdiction string per court — CL's `tax` slug is treated
    // as `us-federal` for now (other slugs map to `us-state` or `us-federal`
    // via map_courtlistener_jurisdiction).
    let court_rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT court_id FROM case_documents ORDER BY court_id",
    )
    .fetch_all(&mut *tx)
    .await
    .context("select distinct court_id")?;

    // court_id (CL slug) -> internal courts.id
    let mut court_ids: BTreeMap<String, Uuid> = BTreeMap::new();
    for (slug,) in &court_rows {
        let jurisdiction = map_courtlistener_jurisdiction(slug);
        let name = canonical_court_name(slug);

        // Upsert via DO UPDATE on the unique key so RETURNING always yields
        // the row id regardless of whether this run inserted or matched.
        // `excluded.source_id = excluded.source_id` keeps the row's data
        // intact on conflict (effectively a no-op write) while still
        // satisfying ON CONFLICT's syntax requirement.
        let inserted_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO courts (tenant_id, name, jurisdiction, source, source_id)
            VALUES ($1, $2, $3, 'courtlistener', $4)
            ON CONFLICT (tenant_id, name) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(&name)
        .bind(jurisdiction)
        .bind(slug)
        .fetch_optional(&mut *tx)
        .await
        .with_context(|| format!("insert court {name}"))?;

        let (id, was_inserted) = match inserted_id {
            Some(id) => (id, true),
            None => {
                let id: Uuid = sqlx::query_scalar(
                    "SELECT id FROM courts WHERE tenant_id = $1 AND name = $2",
                )
                .bind(tenant_id)
                .bind(&name)
                .fetch_one(&mut *tx)
                .await
                .with_context(|| format!("select existing court {name}"))?;
                (id, false)
            }
        };

        court_ids.insert(slug.clone(), id);
        if was_inserted {
            stats.courts_inserted += 1;
        } else {
            stats.courts_existing += 1;
        }
    }

    // ── 2. Judges ──────────────────────────────────────────────────────────
    // Stream opinion headers and accumulate distinct judges keyed by
    // normalized name.  First-court-seen wins for `primary_court_id`.
    let docs: Vec<(String, String)> = sqlx::query_as(
        "SELECT court_id, full_text_plain FROM case_documents",
    )
    .fetch_all(&mut *tx)
    .await
    .context("select case_documents")?;

    stats.case_documents_scanned = docs.len() as u64;

    // normalized_name -> (full_name, primary_court_slug)
    let mut judges_seen: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (court_slug, full_text) in &docs {
        // Look at the opinion header (first ~5 KB) where judge names appear.
        let head = head_chars(full_text, 5_000);
        for full_name in extract_judge_names(&head) {
            let norm = normalize_judge_name(&full_name);
            if norm.is_empty() {
                continue;
            }
            judges_seen
                .entry(norm)
                .or_insert_with(|| (full_name, court_slug.clone()));
        }
    }

    for (normalized, (full_name, court_slug)) in &judges_seen {
        let primary_court_id = court_ids.get(court_slug).copied();

        let inserted_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO judges (
                tenant_id, full_name, normalized_name, primary_court_id, source
            )
            VALUES ($1, $2, $3, $4, 'courtlistener')
            ON CONFLICT (tenant_id, normalized_name) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(full_name)
        .bind(normalized)
        .bind(primary_court_id)
        .fetch_optional(&mut *tx)
        .await
        .with_context(|| format!("insert judge {full_name}"))?;

        if inserted_id.is_some() {
            stats.judges_inserted += 1;
        } else {
            stats.judges_existing += 1;
        }
    }

    tx.commit().await.context("commit populate-kg tx")?;
    Ok(stats)
}

/// Map a CourtListener court slug to a coarse jurisdiction string.
///
/// First-pass: the only slug currently ingested is `tax`.  Future slugs are
/// classified via prefix conventions; unrecognised slugs default to `unknown`
/// rather than guessing.
pub(crate) fn map_courtlistener_jurisdiction(slug: &str) -> &'static str {
    match slug {
        // US Tax Court is a federal Article-I court.
        "tax" => "us-federal",
        // CourtListener federal slugs: scotus, ca1..ca11, cafc, cadc, dcd, ...
        s if s == "scotus" => "us-federal",
        s if s.starts_with("ca") => "us-federal",
        s if s.ends_with("d") && s.len() == 4 => "us-federal", // e.g. nyed, casd
        // CL state slugs are typically 2-letter prefixes (cal, ny, tx, ...).
        s if s.len() == 2 || s.len() == 3 => "us-state",
        _ => "unknown",
    }
}

/// Human-readable court name for a CL slug.
pub(crate) fn canonical_court_name(slug: &str) -> String {
    match slug {
        "tax" => "United States Tax Court".to_string(),
        "scotus" => "Supreme Court of the United States".to_string(),
        // Fallback: prefix with "Court (" + slug + ")" so the row is
        // distinguishable even before S5.7 NLP catches up.
        _ => format!("Court ({slug})"),
    }
}

/// Truncate `s` to the first `max_chars` Unicode characters.
fn head_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// Extract candidate judge names from an opinion header.
///
/// Looks for two common Tax-Court patterns:
///   1. `NAME, Judge:`              (line-leading proper-name list)
///   2. `JUDGE NAME.` / `NAME J.`   (caps-only signatures)
///
/// Returns each unique candidate in source order; the caller normalizes.
pub fn extract_judge_names(text: &str) -> Vec<String> {
    let mut hits = BTreeSet::new();
    let mut ordered = Vec::new();
    for line in text.lines() {
        for cand in extract_judges_from_line(line) {
            if hits.insert(cand.clone()) {
                ordered.push(cand);
            }
        }
    }
    ordered
}

/// Line-level extractor — small, dependency-free.
///
/// Recognised shapes (case-sensitive):
///
///   Tax court:
///     * `NAME, Judge:`                → captures NAME
///     * `NAME, Judge.`                → captures NAME
///     * `NAME, J., delivered`         → captures NAME
///     * `JUDGE NAME delivered`        → captures NAME (caps-only)
///
///   SCOTUS (Sprint 16):
///     * `NAME, C. J.` / `NAME, Ch. J.` → captures NAME (Chief Justice
///                                        signature, early-era SCOTUS)
///     * `NAME, J.` (alone on a line)   → captures NAME (Associate Justice)
///     * `JUSTICE NAME delivered`       → captures NAME
///     * `Chief Justice NAME delivered` → captures NAME
///     * `Mr. Justice NAME delivered`   → captures NAME (pre-1980 SCOTUS)
///
///   Federal-circuit panels (Sprint 16):
///     * `Before X, Y, and Z, Circuit Judges`     → captures X, Y, Z
///     * `Before X, Chief Judge, Y and Z, ...`    → captures X, Y, Z
///     * `NAME, Circuit Judge.` (opinion author)  → captures NAME
///
/// Anything else returns empty. Names are accepted as 1–3 whitespace-
/// separated word-tokens of letters/hyphens/apostrophes — broader patterns
/// would need a real grammar.
fn extract_judges_from_line(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.len() > 200 {
        return Vec::new();
    }
    let mut out = Vec::new();

    // ── Federal-circuit panels: `Before X, Y, and Z, Circuit Judges` ──
    // Handled first so the comma-list doesn't fall through to the
    // single-name `, J.` markers.
    if let Some(names) = extract_circuit_panel(trimmed) {
        out.extend(names);
        return out;
    }

    // ── Tax-court / SCOTUS Justice signatures ──
    // Pattern 1-5: `<name>, Judge:` / `, Judge.` / `, J., delivered` / `, J.,`
    //              / `, C. J.` / `, Ch. J.` / `, Circuit Judge.`
    // Break after the first marker hit so a line like
    // `Holmes, J., delivered the opinion` (which contains both `, J., delivered`
    // and `, J.,` as substrings) yields one candidate, not two.
    for marker in [
        ", Judge:",
        ", Judge.",
        ", J., delivered",
        ", J.,",
        ", C. J.",        // SCOTUS Chief Justice (modern abbreviation)
        ", Ch. J.",       // SCOTUS Chief Justice (early-era abbreviation)
        ", Circuit Judge.",
    ] {
        if let Some(idx) = trimmed.find(marker) {
            let candidate = trimmed[..idx].trim();
            if is_proper_name_candidate(candidate) {
                out.push(candidate.to_string());
                break;
            }
        }
    }

    // SCOTUS line of the form `NAME, J.` standing alone (no `delivered`
    // and no trailing comma after the period). Catches early-era
    // associate-justice signatures like `JOHNSON, J.`. Skip if anything
    // already matched on this line.
    if out.is_empty() && trimmed.ends_with(", J.") {
        let candidate = trimmed[..trimmed.len() - 4].trim();
        if is_proper_name_candidate(candidate) {
            out.push(candidate.to_string());
        }
    }

    // Pattern 4 (existing): `JUDGE <NAME> delivered ...`
    if out.is_empty() {
        if let Some(rest) = trimmed.strip_prefix("JUDGE ") {
            let name = rest
                .split_whitespace()
                .take_while(|w| w.chars().all(|c| c.is_uppercase() || c == '.' || c == ','))
                .collect::<Vec<_>>()
                .join(" ");
            let name = name.trim_end_matches(',').trim_end_matches('.');
            if !name.is_empty() && is_proper_name_candidate(name) {
                out.push(name.to_string());
            }
        }
    }

    // SCOTUS modern: `JUSTICE NAME delivered`, `Chief Justice NAME delivered`,
    // `Mr. Justice NAME delivered`. Captures NAME (caps-only, 1-2 tokens).
    if out.is_empty() {
        for prefix in ["JUSTICE ", "Chief Justice ", "Mr. Justice ", "Mr. Chief Justice "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let name = rest
                    .split_whitespace()
                    .take_while(|w| w.chars().all(|c| c.is_uppercase() || c == '.' || c == ','))
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ");
                let name = name.trim_end_matches(',').trim_end_matches('.');
                if !name.is_empty() && is_proper_name_candidate(name) {
                    out.push(name.to_string());
                    break;
                }
            }
        }
    }

    out
}

/// Parse a `Before X, Y, and Z, Circuit Judges` panel header. Returns the
/// list of panellist surnames (in source order) or None if the line isn't a
/// recognized panel header.
///
/// Accepts the common variations:
///   * `Before DYK, REYNA, and STOLL, Circuit Judges`
///   * `Before LOURIE, STOLL, and STARK, Circuit Judges.`
///   * `Before: TYMKOVICH, Chief Judge, KELLY and PHILLIPS, Circuit Judges`
///   * `Before TYMKOVICH, C. J., KELLY and PHILLIPS, Circuit Judges`
fn extract_circuit_panel(line: &str) -> Option<Vec<String>> {
    let rest = line.strip_prefix("Before:").or_else(|| line.strip_prefix("Before"))?;
    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == ':');

    // Find the "Circuit Judges" or "Circuit Judge" terminator — anything past
    // it isn't a panellist.
    let end = rest.find(", Circuit Judge").or_else(|| rest.find("Circuit Judge"))?;
    let panel_text = &rest[..end].trim_end_matches(|c: char| c == ',' || c == ' ');

    // Split on commas and "and". Drop tokens that are role labels
    // ("Chief Judge", "C. J.", "Senior Circuit Judge", "District Judge").
    let mut names = Vec::new();
    for raw in panel_text.split(|c: char| c == ',') {
        for chunk in raw.split(" and ") {
            let token = chunk.trim().trim_end_matches('.').trim();
            if token.is_empty() {
                continue;
            }
            // Skip role labels — they're not panellist names.
            let lower = token.to_lowercase();
            if lower == "chief judge"
                || lower == "c. j."
                || lower == "ch. j."
                || lower == "senior circuit judge"
                || lower == "district judge"
                || lower == "senior judge"
            {
                continue;
            }
            if is_proper_name_candidate(token) {
                names.push(token.to_string());
            }
        }
    }
    if names.is_empty() {
        return None;
    }
    Some(names)
}

/// Return true if `s` looks like a tax-court judge name in opinion-preamble
/// shape.  The corpus uses two narrow conventions:
///   * single capitalized token (`Holmes`, `GOEKE`)
///   * all-uppercase multi-token strings (`PATRICIA A. TORRES`)
///
/// Mixed-case multi-token strings like `Some Random Words` are rejected
/// because they don't match either real-world shape and tend to be
/// false-positives off non-preamble lines.
fn is_proper_name_candidate(s: &str) -> bool {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.is_empty() || tokens.len() > 3 {
        return false;
    }
    // All chars must be letters / `-` / `'` / `.`
    for t in &tokens {
        for c in t.chars() {
            if !(c.is_alphabetic() || c == '-' || c == '\'' || c == '.') {
                return false;
            }
        }
    }
    // Single token: just needs an uppercase letter.
    if tokens.len() == 1 {
        return tokens[0].chars().any(|c| c.is_uppercase());
    }
    // Multi-token: every token must be all-uppercase (titles/initials count).
    // This rejects mixed-case false-positives like "Some Random Words".
    tokens.iter().all(|t| {
        let letters: String = t.chars().filter(|c| c.is_alphabetic()).collect();
        !letters.is_empty() && letters.chars().all(|c| c.is_uppercase())
    })
}

/// Lowercase, strip titles, collapse whitespace.  This is the canonical
/// match key for `judges.normalized_name`.
pub fn normalize_judge_name(raw: &str) -> String {
    let s = raw.to_lowercase();
    // Strip leading honorifics.
    let s = s
        .trim_start_matches("hon. ")
        .trim_start_matches("hon ")
        .trim_start_matches("judge ")
        .trim();
    // Collapse internal whitespace.
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ────────────────────────────────────────────────────────────────────────────
// Sprint 16 / S16.3 — Attorney-name extraction.
// ────────────────────────────────────────────────────────────────────────────

/// Which side of `v.` an attorney appears for. Drives the win-rate rollup
/// in `extract::run_extraction`: an attorney for the petitioner whose
/// case's `outcome_for == petitioner` is a "win for petitioner".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttorneySide {
    Petitioner,
    Respondent,
}

/// Extract candidate (attorney_name, side) tuples from an opinion's
/// counsel block. Conservative: a pattern only fires when both the name
/// AND the side label are unambiguous. Recall is intentionally low —
/// mislabelling an attorney's side flips the win-rate sign for that
/// attorney, which is worse for the downstream model than skipping them.
///
/// Three corpus shapes are supported (tuned against the live dev DB —
/// 5,109 docs across tax/cafc/us):
///
///   * **Tax court / Tax-court-style counsel block:**
///     ```text
///     George B. Abney and Brody A. Klett, for petitioner.
///     Aimee R. Lobo-Berg, ... for respondent.
///     ```
///     One line per side, names separated by `,` or ` and `.
///
///   * **CAFC (Federal Circuit) counsel block:**
///     ```text
///     WILLIAM PETERSON RAMEY, III, Ramey LLP, Houston, TX, argued for
///     plaintiff-appellant ...
///     MICHAEL I. SANTUCCI, 500law, Fort Lauderdale, FL, argued for
///     defendant-appellee. Also represented by SALVATORE FAZIO; ...
///     ```
///     Lead name in CAPS at the start of a paragraph, terminated by the
///     first comma; side determined by the `for <role>` clause that
///     follows.
///
///   * **Old CAP / early SCOTUS counsel block:**
///     ```text
///     Mr. Tilghman, for the plaintiffs in error.
///     Mr. Ingersoll, for the defendant.
///     ```
///     `Mr. NAME, for the <side>.` — plaintiff(s) in error == petitioner,
///     defendant(s) in error == respondent.
pub fn extract_attorney_names(text: &str) -> Vec<(String, AttorneySide)> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    // Stitch wrapped lines: CAFC counsel paragraphs wrap mid-sentence,
    // so a `for plaintiff-appellant` clause might be on the line AFTER
    // the lead attorney name. Join paragraphs (consecutive non-empty
    // lines) into single logical lines.
    for paragraph in split_paragraphs(text) {
        for (name, side) in extract_attorneys_from_paragraph(&paragraph) {
            let key = (name.clone(), side);
            if seen.insert(key) {
                out.push((name, side));
            }
        }
    }
    out
}

/// Group consecutive non-blank lines into single "paragraphs" so the
/// attorney scanner sees a wrapped CAFC counsel paragraph as one line.
fn split_paragraphs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(trimmed);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Per-paragraph scanner. Returns the (name, side) tuples found in this
/// paragraph. A paragraph that doesn't end in `for <side>.` returns
/// empty — we treat the side label as required.
fn extract_attorneys_from_paragraph(paragraph: &str) -> Vec<(String, AttorneySide)> {
    let mut out = Vec::new();
    // Cap on paragraph length — counsel blocks are short. Anything
    // longer than 600 chars is almost certainly the opinion body and
    // would yield noisy false positives (e.g. "The Court has held ...
    // ... for petitioner ...").  Loosened from 800 → 600 after the
    // S16.3 dev-DB probe surfaced false positives from long body
    // paragraphs containing both a leading proper-noun and an
    // incidental `for petitioner` later in the same paragraph.
    if paragraph.len() > 600 {
        return out;
    }

    // Find the side label first — if there isn't one, bail.
    let side = match find_side_label(paragraph) {
        Some(s) => s,
        None => return out,
    };

    // Find the slice of the paragraph BEFORE the side label — that's
    // where the names live.
    let label_idx = paragraph
        .to_ascii_lowercase()
        .find("for ")
        .map(|i| {
            // Find the `for` that's actually the side-label `for ...`,
            // not a stray earlier `for` in a firm name. Scan forward
            // through every match and pick the one whose tail matches
            // a known side.
            let lower = paragraph.to_ascii_lowercase();
            let mut best = i;
            for (idx, _) in lower.match_indices("for ") {
                let tail = &lower[idx..];
                if tail_indicates_side(tail).is_some() {
                    best = idx;
                    break;
                }
            }
            best
        })
        .unwrap_or(paragraph.len());

    let head = paragraph[..label_idx].trim().trim_end_matches(|c: char| {
        c == ',' || c == ';' || c == ':' || c == '.' || c.is_whitespace()
    });

    // Old CAP form: `Mr. Tilghman` or `Mr. Tilghman and Mr. Foo`.
    // Detect by leading `Mr. ` or `Messrs.` and extract the surname(s).
    if let Some(names) = extract_old_cap_names(head) {
        for n in names {
            out.push((n, side));
        }
        return out;
    }

    // CAFC form: lead attorney name is the FIRST all-caps chunk at the
    // very start of the paragraph, terminated by the first comma.
    // Heuristic: if `head` starts with two or more consecutive
    // all-uppercase tokens, take everything up to the first comma as
    // the lead name.
    if let Some(name) = extract_cafc_lead_name(head) {
        out.push((name, side));
        return out;
    }

    // Tax-court form: `<Name1>, <Name2>, and <Name3>` — mixed case,
    // comma-and-`and`-separated. Split + filter to proper-name shapes.
    for name in split_tax_counsel_names(head) {
        out.push((name, side));
    }
    out
}

/// Identify the side label at the END of a counsel paragraph. Recognised
/// idioms (case-insensitive):
///   * `for petitioner(s)`            → Petitioner
///   * `for the petitioner(s)`        → Petitioner
///   * `for plaintiff(s)` / `-appellant` → Petitioner
///   * `for appellant(s)`             → Petitioner
///   * `for the plaintiff(s) in error`→ Petitioner
///   * `for respondent(s)`            → Respondent
///   * `for the respondent(s)`        → Respondent
///   * `for defendant(s)` / `-appellee` → Respondent
///   * `for appellee(s)`              → Respondent
///   * `for the defendant(s) in error`→ Respondent
fn find_side_label(paragraph: &str) -> Option<AttorneySide> {
    let lower = paragraph.to_ascii_lowercase();
    // Walk every `for ` occurrence and pick the first one whose tail
    // points at a known side label. A counsel paragraph that mentions
    // "Pillsbury Winthrop Shaw Pittman, of Los Angeles, California,
    // argued for plaintiff-appellant" has two `for`s (the firm clause
    // and the side clause) — we want the SIDE one.
    for (idx, _) in lower.match_indices("for ") {
        let tail = &lower[idx..];
        if let Some(side) = tail_indicates_side(tail) {
            return Some(side);
        }
    }
    None
}

/// Given a lowercase `for ...` tail, classify the side or return None
/// if the tail isn't one of the recognised idioms.
fn tail_indicates_side(tail: &str) -> Option<AttorneySide> {
    // Tail must start with `for `; everything after is the side phrase.
    let after = tail.strip_prefix("for ")?;
    // Strip an optional leading `the `.
    let after = after.strip_prefix("the ").unwrap_or(after);

    // Petitioner side labels — alphabetised.
    const PET_LABELS: &[&str] = &[
        "appellant",
        "appellants",
        "petitioner",
        "petitioners",
        "plaintiff",
        "plaintiff-appellant",
        "plaintiff-appellants",
        "plaintiffs",
        "plaintiffs in error",
        "plaintiff in error",
    ];
    // Respondent side labels.
    const RESP_LABELS: &[&str] = &[
        "appellee",
        "appellees",
        "defendant",
        "defendant-appellee",
        "defendant-appellees",
        "defendants",
        "defendants in error",
        "defendant in error",
        "respondent",
        "respondents",
    ];

    // Longest-match-first across both label sets so `plaintiff-appellant`
    // matches before `plaintiff`.
    let mut all: Vec<(&str, AttorneySide)> = Vec::new();
    for l in PET_LABELS {
        all.push((*l, AttorneySide::Petitioner));
    }
    for l in RESP_LABELS {
        all.push((*l, AttorneySide::Respondent));
    }
    all.sort_by_key(|(l, _)| std::cmp::Reverse(l.len()));

    for (label, side) in all {
        if after.starts_with(label) {
            // Require the label to be word-terminated (next char is
            // non-alphabetic) so `for petitioners` doesn't get matched
            // by the `petitioner` prefix in an unrelated context.
            let next = after.as_bytes().get(label.len()).copied().unwrap_or(b'.');
            if !(next as char).is_alphabetic() {
                return Some(side);
            }
        }
    }
    None
}

/// Recognise the old-CAP `Mr. SURNAME` (or `Messrs. A and B`) form.
/// Returns `Some(surnames)` if matched, else None.  The honorific is
/// stripped from each returned name (`Mr. Tilghman` → `Tilghman`).
///
/// Single-token surnames are accepted here (relaxed from the general
/// `is_attorney_name_candidate` 2-token floor) because the
/// pre-1900 CAP idiom is canonically `Mr. <Surname>`.
fn extract_old_cap_names(head: &str) -> Option<Vec<String>> {
    let trimmed = head.trim();
    if let Some(rest) = trimmed.strip_prefix("Messrs. ") {
        // `Messrs. Tilghman and Ingersoll` — split on `and` / `,`.
        let mut names = Vec::new();
        for chunk in rest.split(|c: char| c == ',').flat_map(|c| c.split(" and ")) {
            let name = chunk.trim().trim_end_matches('.').trim();
            if is_old_cap_surname_candidate(name) {
                names.push(name.to_string());
            }
        }
        if names.is_empty() {
            None
        } else {
            Some(names)
        }
    } else if let Some(rest) = trimmed.strip_prefix("Mr. ") {
        // `Mr. Tilghman` or `Mr. Tilghman and Mr. Ingersoll`.
        let mut names = Vec::new();
        for chunk in rest.split(" and Mr. ") {
            let name = chunk.trim().trim_end_matches('.').trim();
            if is_old_cap_surname_candidate(name) {
                names.push(name.to_string());
            }
        }
        if names.is_empty() {
            None
        } else {
            Some(names)
        }
    } else {
        None
    }
}

/// Looser shape check for the old-CAP `Mr. <Surname>` idiom: 1-3
/// alphabetic tokens, starting uppercase, ≥3 letters total.
fn is_old_cap_surname_candidate(s: &str) -> bool {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.is_empty() || tokens.len() > 3 {
        return false;
    }
    let mut has_upper_start = false;
    let mut letter_count = 0usize;
    for t in &tokens {
        for c in t.chars() {
            if !(c.is_alphabetic() || c == '-' || c == '\'') {
                return false;
            }
            if c.is_alphabetic() {
                letter_count += 1;
            }
        }
        if t.chars().next().map_or(false, |c| c.is_uppercase()) {
            has_upper_start = true;
        }
    }
    has_upper_start && letter_count >= 3
}

/// Recognise the CAFC `LEAD NAME, Firm, City, ST` form. Returns the
/// lead name if the paragraph starts with at least two consecutive
/// all-uppercase tokens; otherwise None.  Stitches name-suffix tokens
/// (`JR`, `JR.`, `II`, `III`, `IV`, `SR.`) past the first comma so
/// `WILLIAM PETERSON RAMEY, III, Ramey LLP` returns `RAMEY III`
/// included in the lead name.
fn extract_cafc_lead_name(head: &str) -> Option<String> {
    let first_comma = head.find(',')?;
    let lead = head[..first_comma].trim();
    let tokens: Vec<&str> = lead.split_whitespace().collect();
    if tokens.len() < 2 || tokens.len() > 5 {
        return None;
    }
    // Require all tokens to be ALL-CAPS (letters/digits/punct only).
    // This guards against tax-court mixed-case names misfiring here.
    for t in &tokens {
        let letters: String = t.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.is_empty() || letters.chars().any(|c| c.is_lowercase()) {
            return None;
        }
    }

    // Peek past the first comma for a generation suffix.
    let rest_after_comma = head[first_comma + 1..].trim_start();
    let next_comma = rest_after_comma.find(',').unwrap_or(rest_after_comma.len());
    let next_chunk = rest_after_comma[..next_comma].trim();
    let next_normalized = next_chunk.trim_end_matches('.');
    const SUFFIXES: &[&str] = &["JR", "JR.", "SR", "SR.", "II", "III", "IV"];
    let full_lead = if SUFFIXES.iter().any(|s| s.eq_ignore_ascii_case(next_normalized)) {
        format!("{lead} {next_normalized}")
    } else {
        lead.to_string()
    };

    if is_attorney_name_candidate(&full_lead) {
        Some(full_lead)
    } else {
        None
    }
}

/// Split a tax-court counsel name list on `,` and ` and `. Filters
/// to tokens that look like proper attorney names.
fn split_tax_counsel_names(head: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    // Split on `,` first, then on ` and ` within each chunk.
    for raw_chunk in head.split(|c: char| c == ',') {
        for sub in raw_chunk.split(" and ") {
            let name = sub.trim().trim_end_matches('.').trim();
            if is_attorney_name_candidate(name) && seen.insert(name.to_string()) {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// True if `s` looks like an attorney's full name. Conservative —
/// requires 2-5 tokens of letters/initials/hyphens/apostrophes, at
/// least one of which starts with an uppercase letter.
fn is_attorney_name_candidate(s: &str) -> bool {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() < 2 || tokens.len() > 5 {
        return false;
    }
    let mut has_upper_start = false;
    for t in &tokens {
        if t.is_empty() {
            return false;
        }
        for c in t.chars() {
            if !(c.is_alphabetic() || c == '-' || c == '\'' || c == '.' || c == ',') {
                return false;
            }
        }
        if t.chars().next().map_or(false, |c| c.is_uppercase()) {
            has_upper_start = true;
        }
    }
    if !has_upper_start {
        return false;
    }
    // Reject all-numeric / single-letter degenerate forms.
    let letter_count: usize = tokens
        .iter()
        .map(|t| t.chars().filter(|c| c.is_alphabetic()).count())
        .sum();
    if letter_count < 3 {
        return false;
    }
    // Reject obvious non-names (role labels / generic phrases) that
    // snuck through the splitter. These appear after `for the` in
    // old-CAP / CAFC text, or as paragraph-leading prose like
    // "The Court has previously held..." that happens to also contain
    // a "for petitioner" phrase later in the same paragraph.
    let lower = s.to_ascii_lowercase();
    const NON_NAME_TOKENS: &[&str] = &[
        "law firm",
        "et al",
        "attorney general",
        "solicitor general",
        "assistant attorney",
        "office of",
        "department of",
        "united states",
        // Common prose openers — these are not attorney names but
        // can pass the structural check.
        "the court",
        "this court",
        "the commissioner",
        "the petitioner",
        "the respondent",
        "the secretary",
        "the government",
        "the district",
        "the appellant",
        "the appellee",
        "the defendant",
        "the plaintiff",
    ];
    for marker in NON_NAME_TOKENS {
        if lower == *marker || lower.starts_with(&format!("{marker} ")) {
            return false;
        }
    }
    // Reject candidates that contain lowercase common stopwords as
    // the leading token (`the`, `of`, `for`, `and`, `with`) — those
    // are mid-sentence captures, not names.
    if let Some(first) = tokens.first() {
        let first_lower = first.to_ascii_lowercase();
        const STOPWORD_LEADERS: &[&str] = &["the", "of", "for", "and", "with", "by", "this", "that"];
        if STOPWORD_LEADERS.contains(&first_lower.as_str()) {
            return false;
        }
    }
    true
}

/// Canonical match key for `attorneys.normalized_name`. Lowercase,
/// punctuation-stripped, whitespace-collapsed.
pub fn normalize_attorney_name(raw: &str) -> String {
    let lowered = raw.to_lowercase();
    // Strip leading honorifics.
    let stripped = lowered
        .trim_start_matches("mr. ")
        .trim_start_matches("mr ")
        .trim_start_matches("ms. ")
        .trim_start_matches("ms ")
        .trim_start_matches("mrs. ")
        .trim_start_matches("dr. ")
        .trim();
    // Drop generation suffixes (`, jr.` / `, iii`) and strip stray
    // commas/periods.
    let cleaned: String = stripped
        .chars()
        .map(|c| if c == ',' || c == '.' { ' ' } else { c })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jurisdiction_tax_is_federal() {
        assert_eq!(map_courtlistener_jurisdiction("tax"), "us-federal");
        assert_eq!(map_courtlistener_jurisdiction("scotus"), "us-federal");
        assert_eq!(map_courtlistener_jurisdiction("ca9"), "us-federal");
    }

    #[test]
    fn canonical_court_names() {
        assert_eq!(canonical_court_name("tax"), "United States Tax Court");
        assert_eq!(
            canonical_court_name("unknown-court"),
            "Court (unknown-court)"
        );
    }

    #[test]
    fn extracts_judge_from_line_leading_pattern() {
        let line = "GOEKE, Judge: This case is before us on petitioner's";
        assert_eq!(extract_judges_from_line(line), vec!["GOEKE".to_string()]);
    }

    #[test]
    fn extracts_judge_from_initial_pattern() {
        let line = "Holmes, J., delivered the opinion of the Court.";
        assert_eq!(extract_judges_from_line(line), vec!["Holmes".to_string()]);
    }

    #[test]
    fn rejects_non_name_lines() {
        // Too many tokens
        assert!(extract_judges_from_line(
            "Some Random Words, Judge: appearing here"
        )
        .is_empty());
        // Empty + overly long lines
        assert!(extract_judges_from_line("").is_empty());
        assert!(extract_judges_from_line(&"x".repeat(250)).is_empty());
    }

    #[test]
    fn normalize_strips_honorifics() {
        assert_eq!(normalize_judge_name("Hon. Jane Smith"), "jane smith");
        assert_eq!(normalize_judge_name("Judge GOEKE"), "goeke");
        assert_eq!(normalize_judge_name("  Holmes  "), "holmes");
    }

    #[test]
    fn extract_judge_names_is_dedup_and_ordered() {
        // Same name twice → emitted once; different names preserve insertion order.
        let header = "
            GOEKE, Judge: This case is before us.
            Holmes, J., delivered the opinion of the Court.
            GOEKE, Judge: (reiterated)
        ";
        let names = extract_judge_names(header);
        assert_eq!(names, vec!["GOEKE".to_string(), "Holmes".to_string()]);
    }

    // ── Sprint 16 / S16.2 — SCOTUS + circuit panel patterns ──────────────

    #[test]
    fn extracts_scotus_chief_justice_early_era() {
        // Real CAP early-SCOTUS opinion: "Marshall, Ch. J." on its own line.
        let line = "Marshall, Ch. J.";
        assert_eq!(extract_judges_from_line(line), vec!["Marshall".to_string()]);
    }

    #[test]
    fn extracts_scotus_chief_justice_modern_abbrev() {
        let line = "Roberts, C. J., delivered the opinion of the Court.";
        assert_eq!(extract_judges_from_line(line), vec!["Roberts".to_string()]);
    }

    #[test]
    fn extracts_scotus_associate_justice_standalone() {
        // Old-era: "JOHNSON, J." alone on a line.
        let line = "JOHNSON, J.";
        assert_eq!(extract_judges_from_line(line), vec!["JOHNSON".to_string()]);
    }

    #[test]
    fn extracts_scotus_modern_justice_signature() {
        let line = "JUSTICE BREYER delivered the opinion of the Court.";
        assert_eq!(extract_judges_from_line(line), vec!["BREYER".to_string()]);
    }

    #[test]
    fn extracts_scotus_mr_justice_old_form() {
        let line = "Mr. Justice HOLMES delivered the opinion of the Court.";
        assert_eq!(extract_judges_from_line(line), vec!["HOLMES".to_string()]);
    }

    #[test]
    fn extracts_cafc_panel_three_judges() {
        // The canonical Federal Circuit panel header.
        let line = "Before DYK, REYNA, and STOLL, Circuit Judges.";
        assert_eq!(
            extract_judges_from_line(line),
            vec!["DYK".to_string(), "REYNA".to_string(), "STOLL".to_string()]
        );
    }

    #[test]
    fn extracts_cafc_panel_with_colon_after_before() {
        let line = "Before: LOURIE, STOLL, and STARK, Circuit Judges";
        assert_eq!(
            extract_judges_from_line(line),
            vec!["LOURIE".to_string(), "STOLL".to_string(), "STARK".to_string()]
        );
    }

    #[test]
    fn extracts_cafc_opinion_author_signature() {
        let line = "STOLL, Circuit Judge.";
        assert_eq!(extract_judges_from_line(line), vec!["STOLL".to_string()]);
    }

    #[test]
    fn cafc_panel_skips_chief_judge_role_label() {
        // Tenth-Circuit-style header where one panellist also has a role label.
        let line = "Before TYMKOVICH, Chief Judge, KELLY and PHILLIPS, Circuit Judges.";
        let names = extract_judges_from_line(line);
        // Role label "Chief Judge" must be dropped; panellists remain.
        assert!(names.contains(&"TYMKOVICH".to_string()));
        assert!(names.contains(&"KELLY".to_string()));
        assert!(names.contains(&"PHILLIPS".to_string()));
        assert!(!names.iter().any(|n| n.to_lowercase().contains("chief")));
    }

    #[test]
    fn full_extractor_picks_up_cafc_panel() {
        // End-to-end via extract_judge_names: a real CAFC opening block.
        let text = "\
        United States Court of Appeals\n\
        for the Federal Circuit\n\
        Before DYK, MAYER, and TARANTO, Circuit Judges.\n\
        TARANTO, Circuit Judge.\n\
        ";
        let names = extract_judge_names(text);
        assert!(names.contains(&"DYK".to_string()));
        assert!(names.contains(&"MAYER".to_string()));
        assert!(names.contains(&"TARANTO".to_string()));
    }

    // ── Sprint 16 / S16.3 — attorney extractor ───────────────────────────

    #[test]
    fn attorney_taxcourt_petitioner_two_names() {
        // Real tax-court counsel block.
        let text = "George B. Abney and Brody A. Klett, for petitioner.";
        let names = extract_attorney_names(text);
        assert_eq!(
            names,
            vec![
                ("George B. Abney".to_string(), AttorneySide::Petitioner),
                ("Brody A. Klett".to_string(), AttorneySide::Petitioner),
            ]
        );
    }

    #[test]
    fn attorney_taxcourt_respondent_three_names() {
        let text = "Aimee R. Lobo-Berg, Michelle R. Weigelt, and Jeremy H. Fetter, for respondent.";
        let names = extract_attorney_names(text);
        let just_names: Vec<&str> = names.iter().map(|(n, _)| n.as_str()).collect();
        assert!(just_names.contains(&"Aimee R. Lobo-Berg"));
        assert!(just_names.contains(&"Michelle R. Weigelt"));
        assert!(just_names.contains(&"Jeremy H. Fetter"));
        assert!(names.iter().all(|(_, s)| *s == AttorneySide::Respondent));
    }

    #[test]
    fn attorney_cafc_plaintiff_appellant() {
        // Real CAFC counsel paragraph (slightly trimmed).
        let text = "WILLIAM PETERSON RAMEY, III, Ramey LLP, Houston, TX, argued \
                    for plaintiff-appellant and sanctioned party-appellant.";
        let names = extract_attorney_names(text);
        assert_eq!(
            names,
            vec![(
                "WILLIAM PETERSON RAMEY III".to_string(),
                AttorneySide::Petitioner
            )]
        );
    }

    #[test]
    fn attorney_cafc_defendant_appellee() {
        let text = "MICHAEL I. SANTUCCI, 500law, Fort Lauderdale, FL, argued for \
                    defendant-appellee.";
        let names = extract_attorney_names(text);
        assert_eq!(
            names,
            vec![(
                "MICHAEL I. SANTUCCI".to_string(),
                AttorneySide::Respondent
            )]
        );
    }

    #[test]
    fn attorney_old_cap_mr_form() {
        let text = "Mr. Tilghman, for the plaintiffs in error.";
        let names = extract_attorney_names(text);
        assert_eq!(
            names,
            vec![("Tilghman".to_string(), AttorneySide::Petitioner)]
        );
    }

    #[test]
    fn attorney_old_cap_defendant_side() {
        let text = "Mr. Ingersoll, for the defendant.";
        let names = extract_attorney_names(text);
        assert_eq!(
            names,
            vec![("Ingersoll".to_string(), AttorneySide::Respondent)]
        );
    }

    #[test]
    fn attorney_multiline_counsel_block() {
        // Two paragraphs (separated by a blank line) — one per side.
        let text = "\
George B. Abney and Brody A. Klett, for petitioner.\n\
\n\
Aimee R. Lobo-Berg, for respondent.\n\
";
        let names = extract_attorney_names(text);
        assert!(names.contains(&("George B. Abney".to_string(), AttorneySide::Petitioner)));
        assert!(names.contains(&("Aimee R. Lobo-Berg".to_string(), AttorneySide::Respondent)));
    }

    #[test]
    fn attorney_ignores_paragraph_without_side_label() {
        // Mention of "petitioner" without the `for ` lead-in shouldn't fire.
        let text = "George B. Abney attended the petitioner's deposition.";
        assert!(extract_attorney_names(text).is_empty());
    }

    #[test]
    fn attorney_ignores_overlong_paragraph() {
        // A paragraph longer than 800 chars is almost certainly opinion
        // body, not a counsel block. The scanner bails to keep precision
        // high.
        let body = "x ".repeat(500);
        let text = format!("{body} George B. Abney, for petitioner.");
        assert!(extract_attorney_names(&text).is_empty());
    }

    #[test]
    fn attorney_rejects_non_name_role_labels() {
        // Role-label-looking text after `for the` should not yield an
        // attorney row.
        let text = "Office of the Attorney General, for the respondent.";
        // `Office of the Attorney General` is filtered out — no rows.
        let names = extract_attorney_names(text);
        assert!(names.is_empty());
    }

    #[test]
    fn normalize_attorney_strips_titles_and_punctuation() {
        assert_eq!(normalize_attorney_name("Mr. Tilghman"), "tilghman");
        assert_eq!(
            normalize_attorney_name("George B. Abney"),
            "george b abney"
        );
        assert_eq!(
            normalize_attorney_name("WILLIAM PETERSON RAMEY, III"),
            "william peterson ramey iii"
        );
    }

    #[test]
    fn attorney_dedup_across_paragraphs() {
        // Same attorney mentioned twice — only one (name, side) row.
        let text = "\
George B. Abney, for petitioner.\n\
\n\
George B. Abney also presented argument, for petitioner.\n\
";
        let names = extract_attorney_names(text);
        assert_eq!(
            names,
            vec![("George B. Abney".to_string(), AttorneySide::Petitioner)]
        );
    }
}
