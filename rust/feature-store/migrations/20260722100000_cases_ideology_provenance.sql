-- Sprint 10 / S10.2 — per-case ideology provenance.
--
-- Adds a nullable JSONB column that snapshots the Tier-A ideology source
-- used at prediction time.  Pre-Sprint-10 cases keep NULL; the UI renders
-- the legacy "available sources" footer for those.
--
-- Shape of `ideology_provenance` (written by graphql_predict::createCase):
--
--   {
--     "source":      "martin_quinn",            -- bonica_dime | martin_quinn | judicial_common_space
--     "release":     "mqs-2023-v1",             -- vintage tag of the upstream drop
--     "raw_score":   -1.43,                     -- score in the source's native scale
--     "term":        1972,                      -- MQ only; null for DIME / JCS
--     "as_of_date":  "2026-07-22",              -- date the resolver used (today if not supplied)
--     "resolved_at": "2026-07-22T10:00:00Z"     -- timestamp the snapshot was taken
--   }
--
-- Null when no source fired (operator typed ideologyDistance manually).
--
-- Compliance pattern: predictions are reproducible.  Re-running this case
-- against today's ideology data may produce a different score because the
-- DIME / MQ / JCS releases have since updated; the printed memo always
-- names the vintage that drove the original recommendation.

BEGIN;

ALTER TABLE cases
  ADD COLUMN IF NOT EXISTS ideology_provenance jsonb;

COMMENT ON COLUMN cases.ideology_provenance IS
$col$
Snapshot of the Tier-A ideology source used at prediction time.  See
Sprint 10 / S10.2 plan doc for the JSONB shape.  NULL when no source
fired or when the case was predicted before Sprint 10 landed.
$col$;

-- Partial index for "show me cases where source = X" audit queries.
-- Sprint 10's compliance workflows want fast lookup by source name.
CREATE INDEX IF NOT EXISTS cases_ideology_source_idx
    ON cases ((ideology_provenance->>'source'))
    WHERE ideology_provenance IS NOT NULL;

COMMIT;
