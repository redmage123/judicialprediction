-- Export labelled case_documents + joined judge severity for v1 training.
--
-- One row per case_document with:
--   * the S5.7 extraction outputs (case_type, outcome_for)
--   * the first matched judge's severity_proxy (LATERAL join — picks the
--     judge whose normalized_name appears first in the opinion header)
--   * full_text_plain for the motion-count regex in build_real_corpus.py
--
-- Runs unbounded over all of case_documents — caller is responsible for
-- setting `SET LOCAL app.current_tenant_id` if RLS is on.

SET app.current_tenant_id = '00000000-0000-0000-0000-000000000001';

WITH cd_judges AS (
    -- Sprint 16 / S16.2 — prefer normalized_name matching so the early-SCOTUS
    -- corpus (which signs opinions "Taney, Ch. J." rather than "Roger Brooke
    -- Taney") joins to the FJC-populated KG. Only judges with severity_proxy
    -- data are eligible: this filters out FJC rows for never-matched judges
    -- and avoids picking an unrelated FJC judge whose surname collides with
    -- a word in the opinion body. Falls back to NULL when no judge matches.
    --
    -- Substring strategy: insist on word-boundary tokens via the ' <name> '
    -- form. Surrounding spaces avoid collisions like "ney" matching inside
    -- "money".
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
        (a.bio->'win_rate_proxy'->>'win_rate')::float8 AS attorney_win_rate
    FROM case_documents cd
    LEFT JOIN LATERAL (
        SELECT *
        FROM judges jj
        WHERE jj.bio ? 'severity_proxy'
          AND (
            -- Prefer the FJC-aliased lowercase last-name match.
            position(' ' || jj.normalized_name || ' '   IN lower(cd.full_text_plain)) > 0
            OR position(' ' || jj.normalized_name || ',' IN lower(cd.full_text_plain)) > 0
            OR position(' ' || jj.normalized_name || '.' IN lower(cd.full_text_plain)) > 0
            -- Then fall through to full_name for any judges whose normalized
            -- form is a multi-word string (e.g. "william cushing").
            OR position(jj.full_name IN cd.full_text_plain) > 0
          )
        ORDER BY (jj.bio->'severity_proxy'->>'cases_analyzed')::int DESC NULLS LAST
        LIMIT 1
    ) j ON true
    LEFT JOIN LATERAL (
        SELECT *
        FROM attorneys aa
        WHERE aa.bio ? 'win_rate_proxy'
          AND position(aa.normalized_name IN lower(cd.full_text_plain)) > 0
        ORDER BY (aa.bio->'win_rate_proxy'->>'cases_analyzed')::int DESC NULLS LAST
        LIMIT 1
    ) a ON true
    WHERE cd.case_type IS NOT NULL
)
SELECT json_agg(row_to_json(cd_judges)) FROM cd_judges;
