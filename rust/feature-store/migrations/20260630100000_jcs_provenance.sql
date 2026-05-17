-- Sprint 9 / S9.2 — Judicial Common Space (JCS) ideology provenance.
--
-- Documentation migration.  GIN index on judges.bio (S7.2) already covers
-- bio ? 'jcs'.  We update the COMMENT ON COLUMN to describe the new
-- sub-document shape.
--
-- Shape of `bio.jcs` (written by rust/jcs-ingest):
--
--   {
--     "jcs": {
--       "score":            -0.41,                        -- scalar, ~[-1, 1]
--       "scale":            "epstein-2018",               -- methodology vintage
--       "release":          "jcs-2018-v1",                -- file vintage
--       "source_id":        "<emqs-judge-id>",
--       "ingested_at":      "2026-07-01T10:00:00Z",
--       "match_confidence": "exact|name_only|last_name+court"
--     }
--   }
--
-- JCS coverage extends Martin-Quinn beyond SCOTUS to federal Circuit and
-- District judges via Epstein/Martin/Quinn/Segal joint-scaling.  Same
-- match-confidence enum as DIME / MQ so the audit footer can render
-- all three uniformly.

BEGIN;

COMMENT ON COLUMN judges.bio IS
$bio$
Per-judge enrichment data, written by ingest workers. Top-level keys:

  - severity_proxy : { severity, cases_analyzed, wins_for_respondent }
                     written by rust/ingest-fetcher.
  - dime           : Bonica DIME campaign-finance ideology cfscore.
                     Sprint 7 / S7.2.
  - mqs            : Martin-Quinn per-term ideal points.  Sprint 8 / S8.2.
                     { scores[], latest_score, latest_term, ... }
  - jcs            : Judicial Common Space (Epstein/Martin/Quinn/Segal).
                     Sprint 9 / S9.2.
                     { score, scale, release, source_id, ingested_at,
                       match_confidence }

Precedence in the gateway's extract_features_from_text resolver:
  MQ (voting-record, SCOTUS only) > JCS (joint-scaled voting record,
  federal Circuit + District) > DIME (campaign-finance proxy, broadest
  coverage).

All sub-documents carry a `release` tag and `ingested_at` timestamp so
predictions can be reproduced from the same enrichment vintage.
$bio$;

COMMIT;
