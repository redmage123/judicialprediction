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
    -- Best-effort match: the first judge in the corpus whose normalized name
    -- substring-matches the document text.  Crude but correct for the tiny
    -- 21-judge KG we have today; will be replaced when S5.6 populates
    -- case_judges edges properly.
    SELECT
        cd.id              AS doc_id,
        cd.opinion_id,
        cd.court_id,
        cd.case_type,
        cd.outcome_for,
        cd.full_text_plain,
        j.full_name        AS judge_name,
        (j.bio->'severity_proxy'->>'severity')::float8 AS judge_severity
    FROM case_documents cd
    LEFT JOIN LATERAL (
        SELECT *
        FROM judges
        WHERE position(full_name IN cd.full_text_plain) > 0
        ORDER BY position(full_name IN cd.full_text_plain) ASC
        LIMIT 1
    ) j ON true
    WHERE cd.case_type IS NOT NULL
)
SELECT json_agg(row_to_json(cd_judges)) FROM cd_judges;
