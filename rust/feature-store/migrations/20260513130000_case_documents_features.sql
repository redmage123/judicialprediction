-- =============================================================================
-- S5.7 — Layer-2 NLP feature columns on case_documents
--
-- Adds two derived columns populated by `ingest-fetcher extract-features`:
--   case_type   — closed enum below, regex-classified from Code-section
--                 references + key phrases.  Useful for case-type-aware
--                 retrieval and dashboards.
--   outcome_for — 'petitioner' / 'respondent' / 'split' / NULL (unresolved,
--                 e.g. "Decision will be entered under Rule 155" — case still
--                 in Rule-155 computation phase).
--
-- Both are nullable; classification runs offline and is allowed to abstain.
-- =============================================================================

ALTER TABLE case_documents
    ADD COLUMN case_type   text
        CHECK (case_type IN (
            'income_tax',
            'innocent_spouse',
            'collection_due_process',
            'whistleblower',
            'estate_tax',
            'gift_tax',
            'partnership',
            'employment_tax',
            'penalty'
        )),
    ADD COLUMN outcome_for text
        CHECK (outcome_for IN ('petitioner', 'respondent', 'split')),
    ADD COLUMN features_extracted_at timestamptz;

-- Indexes only on the typed-enum columns; `features_extracted_at` is mainly
-- used to find rows that still need extraction (`IS NULL`), which is a
-- sequential scan over a small ingest table.
CREATE INDEX idx_case_documents_case_type   ON case_documents (case_type)
    WHERE case_type   IS NOT NULL;
CREATE INDEX idx_case_documents_outcome_for ON case_documents (outcome_for)
    WHERE outcome_for IS NOT NULL;
