-- Sprint 16 / S16.5 — materialize the opinion-author edge at ingest time.
--
-- The S15.2 / S16.2 LATERAL join in
-- `python/ml-inference-svc/scripts/export_real_corpus.sql` matches
-- judges against opinion text with substring searches. Two failure modes:
--
--   1. Common-word collisions: justice surnames like "Marshall", "Brown",
--      "White" appear in opinions OTHER than the ones they authored ("the
--      Marshall Plan", "white-collar"), and the LATERAL `LIMIT 1` picks
--      the first match — attaching a random judge's severity_proxy to the
--      row.
--
--   2. Coverage: many SCOTUS opinions (early-CAP slice) only show the
--      justice's surname in the signature line, and the
--      `position(' name ')` substring may miss word-boundary variants.
--
-- Solution: record the FIRST judge extracted by
-- `crate::kg::extract_judge_names` (which already runs the high-precision
-- signature-line patterns) directly on `case_documents` at extract-features
-- time. The export query then becomes a clean equality join to `judges`,
-- with no LATERAL ambiguity.
--
-- The value stored here is the NORMALIZED name (lower-case last-name or
-- "first last" form, matching `judges.normalized_name`), so the export
-- join is a one-column equality predicate.

BEGIN;

ALTER TABLE case_documents
    ADD COLUMN IF NOT EXISTS primary_judge_name text;

COMMENT ON COLUMN case_documents.primary_judge_name IS
$col$
Sprint 16 / S16.5 — normalized name of the first judge whose signature
appeared in this opinion (per `crate::kg::extract_judge_names` +
`normalize_judge_name`). Joins to `judges.normalized_name` for severity-proxy
lookup. NULL when no signature line was recognized. Populated by
`ingest-fetcher extract-features`.
$col$;

-- Partial index — only meaningful when the column is populated, which is the
-- access pattern from the export query.
CREATE INDEX IF NOT EXISTS idx_case_documents_primary_judge_name
    ON case_documents (primary_judge_name)
    WHERE primary_judge_name IS NOT NULL;

COMMIT;
