-- Sprint 7 / S7.2 — Bonica DIME judge-ideology provenance.
--
-- The `judges.bio` JSONB column already exists (created in the baseline
-- migration). This migration is a *documentation + index* migration:
-- no schema rewrite, but it records the canonical shape of the new
-- `bio.dime` sub-document and adds a partial GIN index so the gateway's
-- "is this judge enriched?" probe is O(log n) instead of a sequential scan.
--
-- Shape of `bio.dime` (written by rust/dime-ingest):
--
--   {
--     "cfscore":          -0.41,                       -- float, ~[-2, 2], lower = more liberal
--     "release":          "dime-2014-judges-v1.0",     -- which DIME drop this came from
--     "source_id":        "<bonica-judge-id>",         -- DIME's own identifier for the judge
--     "ingested_at":      "2026-05-17T10:00:00Z",      -- when our importer wrote this
--     "match_confidence": "exact"                      -- exact | court+name | name_only | fuzzy
--   }
--
-- Sprint 8 will add a sibling `bio.mqs` for Martin-Quinn (time-varying) and
-- sprint 9 a `bio.jcs` for the Judicial Common Space. The partial index
-- below already accepts those keys via the `?` operator.

BEGIN;

-- 1. Documentation. `COMMENT ON COLUMN` so anyone reading `\d+ judges` in
--    psql sees the schema. Sprint 6's `judges.bio` comment (if any) is
--    replaced by this longer one.
COMMENT ON COLUMN judges.bio IS
$bio$
Per-judge enrichment data, written by ingest workers. Top-level keys:

  - severity_proxy : { severity, cases_analyzed, wins_for_respondent }
                     written by rust/ingest-fetcher (per-court win rate
                     for the responding party, used as a calibration
                     proxy for judge_severity).
  - dime           : Bonica DIME campaign-finance ideology cfscore.
                     Sprint 7 / S7.2.  See 20260517100000_dime_provenance.
  - mqs            : Martin-Quinn scores.  Sprint 8.
  - jcs            : Judicial Common Space.  Sprint 9.

All sub-documents carry a `release` tag and `ingested_at` timestamp so
predictions can be reproduced from the same enrichment vintage.
$bio$;

-- 2. Partial GIN index for "judges that have DIME enrichment". GIN is
--    overkill for a single key but trivially reusable for the upcoming
--    `bio.mqs` / `bio.jcs` keys, which will be queried the same way
--    (`bio ? 'mqs'`). One index covers all enrichment lookups.
CREATE INDEX IF NOT EXISTS judges_bio_enrichment_keys_gin
    ON judges USING GIN (bio jsonb_path_ops);

COMMIT;
