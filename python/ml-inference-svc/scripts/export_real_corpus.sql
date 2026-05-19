-- Export labelled case_documents + joined judge severity for v1 training.
--
-- One row per case_document with:
--   * the S5.7 extraction outputs (case_type, outcome_for)
--   * the materialized opinion-author edge (S16.5) — joined directly to
--     `judges.normalized_name` so the severity_proxy attaches to the actual
--     author rather than a random surname collision in the body text
--   * full_text_plain for the motion-count regex in build_real_corpus.py
--
-- Runs unbounded over all of case_documents — caller is responsible for
-- setting `SET LOCAL app.current_tenant_id` if RLS is on.

SET app.current_tenant_id = '00000000-0000-0000-0000-000000000001';

WITH cd_judges AS (
    -- Sprint 16 / S16.5 — the ingest extractor now writes
    -- `case_documents.primary_judge_name` (the normalized name of the first
    -- signature-line judge per `crate::kg::extract_judge_names`). That lets
    -- us drop the substring LATERAL entirely and equality-join to `judges`.
    --
    -- This fixes two failure modes the LATERAL had:
    --   1. Common-word collisions ("Marshall" / "Brown" / "White" appearing
    --      in unrelated body text caused random judges' severity to attach).
    --   2. Coverage in the early-CAP SCOTUS slice where the surname only
    --      appeared on the signature line — `extract_judge_names` already
    --      handles those signature patterns, so the materialized edge is
    --      strictly better-recall than `position()` lookups against
    --      `full_text_plain`.
    --
    -- We still gate on `bio ? 'severity_proxy'` so rows whose author has no
    -- computed severity drop to NULL (the build pipeline neutral-fills).
    SELECT
        cd.id              AS doc_id,
        cd.opinion_id,
        cd.court_id,
        cd.case_type,
        cd.outcome_for,
        cd.full_text_plain,
        j.full_name        AS judge_name,
        (j.bio->'severity_proxy'->>'severity')::float8 AS judge_severity,
        -- S16.4 — appointing_president powers the president-as-ideology
        -- fallback in build_real_corpus.py (see president_ideology.py).
        j.appointing_president AS appointing_president,
        -- S16.3 — attorney win-rate join. Mirrors the judge LATERAL above
        -- but on the new `attorneys` table populated by
        -- rust/ingest-fetcher/src/extract.rs::run_extraction. Match key
        -- is `normalized_name` (lowercase multi-token) substring in the
        -- lowercased opinion text. When multiple attorneys match, pick
        -- the one with the largest `cases_analyzed` so the corpus row
        -- gets the attorney with the richest signal.
        a.full_name        AS attorney_name,
        (a.bio->'win_rate_proxy'->>'win_rate')::float8 AS attorney_win_rate,
        -- S16.6: materiality_score inputs. citation_count is sparse (mostly
        -- populated on CL ingest, not on CAP); length(full_text_plain) is
        -- always set on a real opinion.
        cd.citation_count  AS citation_count,
        length(cd.full_text_plain) AS text_length
    FROM case_documents cd
    -- S16.5 — judges joined via the materialized opinion-author edge,
    -- replacing the body-substring LATERAL that suffered surname collisions.
    LEFT JOIN judges j
        ON j.normalized_name = cd.primary_judge_name
       AND j.tenant_id       = current_setting('app.current_tenant_id')::uuid
       AND j.bio ? 'severity_proxy'
    -- S16.3 attorney LATERAL (kept; no materialized edge for attorneys yet).
    LEFT JOIN LATERAL (
        SELECT *
        FROM attorneys aa
        WHERE aa.bio ? 'win_rate_proxy'
          AND position(aa.normalized_name IN lower(cd.full_text_plain)) > 0
        ORDER BY (aa.bio->'win_rate_proxy'->>'cases_analyzed')::int DESC NULLS LAST
        LIMIT 1
    ) a ON true
    -- Sprint 19 perf: filter to LABELLED rows before the LATERALs run.
    -- The attorney LATERAL does `position(aa.normalized_name IN
    -- lower(cd.full_text_plain))` for every row × every attorney; at
    -- 40K rows × 439 attorneys × 150KB text per row that's 2.6T byte
    -- comparisons and the query stalls. The trainer only consumes
    -- labelled rows anyway (split/null are dropped in build_real_corpus.py),
    -- so push the filter into the CTE.
    WHERE cd.case_type IS NOT NULL
      AND cd.outcome_for IN ('petitioner', 'respondent', 'split')
)
SELECT json_agg(row_to_json(cd_judges)) FROM cd_judges;
