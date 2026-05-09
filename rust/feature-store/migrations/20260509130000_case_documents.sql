-- S2.17 (JP-40) — case_documents: public federal/state opinions ingested
-- from CourtListener bulk dumps.
--
-- INTENTIONALLY NO RLS — public data, accessible to all tenants.
-- This is a deliberate policy choice, not an oversight: case opinions are
-- public records and tenant isolation does not apply.

CREATE TABLE IF NOT EXISTS case_documents (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    court_id        text NOT NULL,
    opinion_id      bigint UNIQUE NOT NULL,
    case_name       text,
    date_filed      date,
    citation_count  integer NOT NULL DEFAULT 0,
    full_text_plain text NOT NULL,
    source          text NOT NULL DEFAULT 'courtlistener',
    source_url      text,
    ingested_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS case_documents_court_date_idx
    ON case_documents (court_id, date_filed DESC);

COMMENT ON TABLE case_documents IS
  'Public federal/state case opinions; NO RLS — accessible to all tenants. Source: CourtListener bulk dumps.';

GRANT SELECT, INSERT, UPDATE ON case_documents TO jp_app;
