-- =============================================================================
-- S6.8 — Persist the NLP feature suggestion alongside the operator's values.
--
-- When createCase receives an optional `opinion_text` payload, the gateway
-- runs the same S5.7/S5.8 extractor and stores its `ExtractedFeatures`
-- result here.  `cases.input_features` already holds the operator's final
-- (possibly hand-edited) values; `cases.nlp_suggestion` holds what the NLP
-- pipeline proposed from the raw opinion text.
--
-- Having both columns on the same row makes later NLP-vs-operator accuracy
-- evaluation a plain SQL diff — no join, no separate eval table.
--
-- Nullable: the vast majority of cases are created without opinion text, so
-- NULL is the common case and means "no suggestion was captured".
-- =============================================================================

ALTER TABLE cases
    ADD COLUMN IF NOT EXISTS nlp_suggestion jsonb;

COMMENT ON COLUMN cases.nlp_suggestion IS
    'S6.8 — ExtractedFeatures JSON from running the S5.7/S5.8 extractor over the createCase opinion_text payload. NULL when the case was created without opinion text. Compare against input_features for NLP-vs-operator accuracy evaluation.';
