-- Sprint 8 / S8.2 — Martin-Quinn per-term ideology provenance.
--
-- Like 20260517100000_dime_provenance, this is a documentation migration.
-- The GIN index on judges.bio (added in S7.2) already covers the
-- `bio ? 'mqs'` probe. We update the COMMENT ON COLUMN to describe the
-- new sub-document shape and that's it.
--
-- Shape of `bio.mqs` (written by rust/mqs-ingest):
--
--   {
--     "mqs": {
--       "scores": [
--         { "term": 1990, "post_mean": -0.41, "post_sd": 0.12 },
--         { "term": 1991, "post_mean": -0.38, "post_sd": 0.11 }
--       ],
--       "latest_score":     -0.38,                 -- post_mean of latest_term
--       "latest_term":      1991,
--       "release":          "mqs-2023-v1",
--       "source_id":        "<mq-justice-id>",
--       "ingested_at":      "2026-06-09T10:00:00Z",
--       "match_confidence": "exact|name_only|last_name+court"
--     }
--   }
--
-- The `latest_*` shortcut keys mean the gateway's hot-path lookup is a
-- single JSONB extraction, not a JSON-array scan. Sprint 9 will reuse
-- the same `scores[]` + `latest_*` shape for JCS (`bio.jcs`).

BEGIN;

COMMENT ON COLUMN judges.bio IS
$bio$
Per-judge enrichment data, written by ingest workers. Top-level keys:

  - severity_proxy : { severity, cases_analyzed, wins_for_respondent }
                     written by rust/ingest-fetcher (per-court win rate
                     for the responding party, used as a calibration
                     proxy for judge_severity).
  - dime           : Bonica DIME campaign-finance ideology cfscore.
                     Sprint 7 / S7.2.
                     { cfscore, release, source_id, ingested_at,
                       match_confidence }
  - mqs            : Martin-Quinn per-term ideal points.  Sprint 8 / S8.2.
                     { scores[], latest_score, latest_term, release,
                       source_id, ingested_at, match_confidence }
  - jcs            : Judicial Common Space.  Sprint 9 (same shape as mqs).

All sub-documents carry a `release` tag and `ingested_at` timestamp so
predictions can be reproduced from the same enrichment vintage.
$bio$;

COMMIT;
