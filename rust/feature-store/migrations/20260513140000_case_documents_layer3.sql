-- =============================================================================
-- S6.3 — Layer-3 NLP enrichment columns on case_documents
--
-- `layer3_features` is a jsonb blob produced by the offline enrichment worker
-- (`python/ml-inference-svc/scripts/enrich_layer3.py`).  Schema is
-- intentionally open-ended for v1 (regex-only extractor); when an
-- English-trained LoRA arrives in Sprint 7+ the worker upgrades to LLM
-- inference and the consumer side (api-gateway extractFeatures) reads the
-- same column shape.
--
-- Expected shape (regex extractor v1):
--   {
--     "extractor_version": "regex-v1",
--     "judges":   [{"name": "LAUBER", "role": "writer"}, ...],
--     "statutes": ["I.R.C. § 6662", "26 U.S.C. § 7345"],
--     "citations": ["Smith v. Commissioner, 142 T.C. 24"],
--     "elements": {
--        "summary_judgment_motion": true,
--        "section_6662_penalty": true,
--        "reasonable_cause_defense": false,
--        "willfulness_finding": false
--     }
--   }
--
-- `layer3_extracted_at` is the worker's idempotency cursor — rows with NULL
-- here are enriched on the next run; rows with non-NULL are skipped unless
-- the worker is invoked with --force.
-- =============================================================================

ALTER TABLE case_documents
    ADD COLUMN layer3_features      jsonb,
    ADD COLUMN layer3_extracted_at  timestamptz;

-- Partial index on the unprocessed slice — the worker's hot query is
-- "give me 100 rows that still need enrichment", which is sequential over a
-- shrinking set.  No need for a full-table index.
CREATE INDEX idx_case_documents_layer3_unprocessed
    ON case_documents (id)
    WHERE layer3_extracted_at IS NULL;
