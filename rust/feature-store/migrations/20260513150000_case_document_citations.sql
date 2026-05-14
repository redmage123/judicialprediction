-- =============================================================================
-- S6.5 — Citation graph for the public CourtListener corpus.
--
-- The S5.5 KG migration shipped `case_citations` (citing_case_id, cited_case_id)
-- with FKs to `cases(id)`.  That table is for operator-created cases — the
-- public CourtListener corpus lives in `case_documents` with its own bigint
-- `opinion_id` as the natural key, and has no UUID overlap with `cases`.
--
-- Rather than churn the deployed `case_citations` schema with nullable parallel
-- columns, we mirror the existing public-corpus pattern (`case_documents` is
-- separate from `cases`; no RLS) and add a parallel edge table keyed on
-- opinion_ids.  A future sprint may merge the two views behind a single
-- citation API; that is explicitly out of scope here.
--
-- NO RLS — public data, accessible to all tenants, matching `case_documents`.
-- =============================================================================

-- -----------------------------------------------------------------------------
-- 1. Capture raw cites alongside each opinion.
-- -----------------------------------------------------------------------------
-- `cites_json` stores the raw `opinions_cited` array from CourtListener's
-- `/opinions/<id>/` endpoint — a list of API URIs like
-- `["https://www.courtlistener.com/api/rest/v4/opinions/12345/", ...]`.
-- `cites_extracted_at` tells the populator which rows are still pending
-- a back-fill fetch.  Both default NULL for the pre-existing corpus.
ALTER TABLE case_documents
    ADD COLUMN cites_json          jsonb,
    ADD COLUMN cites_extracted_at  timestamptz;

-- Partial index speeds up the back-fill worker's "what's still pending" scan.
CREATE INDEX idx_case_documents_cites_unextracted
    ON case_documents (id)
    WHERE cites_extracted_at IS NULL;

-- -----------------------------------------------------------------------------
-- 2. Citation edge table.
-- -----------------------------------------------------------------------------
-- `citation_kind` mirrors the closed set from `case_citations` so future
-- merging stays cheap; CourtListener's raw cites array carries no kind
-- signal, so the populator writes 'cited'.  No self-loops.
CREATE TABLE case_document_citations (
    citing_opinion_id  bigint      NOT NULL REFERENCES case_documents(opinion_id) ON DELETE CASCADE,
    cited_opinion_id   bigint      NOT NULL REFERENCES case_documents(opinion_id) ON DELETE CASCADE,
    citation_kind      text        NOT NULL DEFAULT 'cited'
        CHECK (citation_kind IN ('cited', 'affirmed', 'distinguished', 'overruled')),
    created_at         timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (citing_opinion_id, cited_opinion_id),
    CONSTRAINT case_document_citations_no_self_loop
        CHECK (citing_opinion_id <> cited_opinion_id)
);

-- Both query directions: "what does X cite" and "who cites X".
CREATE INDEX idx_case_document_citations_cited
    ON case_document_citations (cited_opinion_id);

COMMENT ON TABLE case_document_citations IS
    'S6.5 — Directed citation graph over case_documents.opinion_id pairs. NO RLS (public corpus).';

GRANT SELECT, INSERT, UPDATE, DELETE ON case_document_citations TO jp_app;
