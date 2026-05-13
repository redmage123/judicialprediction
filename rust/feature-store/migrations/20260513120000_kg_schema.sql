-- =============================================================================
-- S5.5 — Knowledge-graph foundation (Reasoning Layer 0)
--
-- Spec: v2.14 §6.3 "Layer 0 — Knowledge graph"
-- Sprint 5 plan: "KG schema migration: nodes (judges, courts, cases), edges
-- (heard_by, in_court, cites). Postgres native (no Neo4j yet — Sprint 7+ if
-- scale demands)."
--
-- This migration is purely additive.  It does not touch the existing
-- `cases.judge_name` or `cases.court` free-text columns; S5.6 will populate
-- the new tables from `case_documents` extraction and once coverage is
-- meaningful those columns can be deprecated in a future Sprint.
--
-- Tenant model
--   Every node and edge table carries `tenant_id` and ships with the same
--   `tenant_isolation` RLS policy used by `cases`, `features`, etc.
--   Cross-tenant FKs are still technically possible at INSERT time (FK
--   constraints don't read RLS) but every read/write through the gateway
--   uses `SET LOCAL app.current_tenant_id`, so they are not reachable from
--   production code paths.
-- =============================================================================

-- -----------------------------------------------------------------------------
-- courts — court node
-- -----------------------------------------------------------------------------
-- `parent_court_id` lets us represent appellate hierarchy
-- (Tax Court → Court of Appeals → SCOTUS) without a separate edge.  Limited
-- to a self-FK because the parent relationship is always 1:1 (a court has
-- at most one direct parent).
CREATE TABLE courts (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       uuid        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name            text        NOT NULL,
    short_name      text,
    jurisdiction    text        NOT NULL,
    parent_court_id uuid        REFERENCES courts(id) ON DELETE SET NULL,
    source          text,      -- e.g. 'courtlistener'
    source_id       text,      -- external id within the source (CL court slug)
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);

CREATE INDEX idx_courts_tenant_jurisdiction ON courts (tenant_id, jurisdiction);
-- Hot path for the S5.6 ingest dedup lookup.
CREATE INDEX idx_courts_tenant_source ON courts (tenant_id, source, source_id)
    WHERE source IS NOT NULL;

CREATE TRIGGER trg_courts_updated_at
    BEFORE UPDATE ON courts
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

ALTER TABLE courts ENABLE ROW LEVEL SECURITY;
ALTER TABLE courts FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON courts
    USING      (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);


-- -----------------------------------------------------------------------------
-- judges — judge node
-- -----------------------------------------------------------------------------
-- `normalized_name` is the canonical lookup key used by the S5.7 NLP
-- extractor when matching free-text judge names.  Held lowercase, with
-- titles/punctuation stripped.  UNIQUE per tenant so re-extraction is
-- idempotent.
CREATE TABLE judges (
    id                  uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           uuid        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    full_name           text        NOT NULL,
    normalized_name     text        NOT NULL,
    primary_court_id    uuid        REFERENCES courts(id) ON DELETE SET NULL,
    bio                 jsonb       NOT NULL DEFAULT '{}',
    source              text,
    source_id           text,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, normalized_name)
);

CREATE INDEX idx_judges_tenant_primary_court ON judges (tenant_id, primary_court_id);
CREATE INDEX idx_judges_tenant_source ON judges (tenant_id, source, source_id)
    WHERE source IS NOT NULL;

CREATE TRIGGER trg_judges_updated_at
    BEFORE UPDATE ON judges
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

ALTER TABLE judges ENABLE ROW LEVEL SECURITY;
ALTER TABLE judges FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON judges
    USING      (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);


-- -----------------------------------------------------------------------------
-- case_judges — `heard_by` edge (case → judge, many-to-many)
-- -----------------------------------------------------------------------------
-- A case can have multiple judges (panel decisions, replacements).  `role`
-- captures `presiding` / `panel_member` / `concurring` / `dissenting`.  Null
-- when extraction can't tell.
CREATE TABLE case_judges (
    case_id     uuid        NOT NULL REFERENCES cases(id)  ON DELETE CASCADE,
    judge_id    uuid        NOT NULL REFERENCES judges(id) ON DELETE CASCADE,
    tenant_id   uuid        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    role        text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (case_id, judge_id)
);

CREATE INDEX idx_case_judges_tenant_case  ON case_judges (tenant_id, case_id);
CREATE INDEX idx_case_judges_tenant_judge ON case_judges (tenant_id, judge_id);

ALTER TABLE case_judges ENABLE ROW LEVEL SECURITY;
ALTER TABLE case_judges FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON case_judges
    USING      (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);


-- -----------------------------------------------------------------------------
-- case_courts — `in_court` edge (case → court, many-to-many)
-- -----------------------------------------------------------------------------
-- Modeled as an edge rather than a `cases.court_id` FK because the same case
-- can travel through multiple courts (transfer, appeal, MDL consolidation).
-- The original court is marked `is_primary = true` so dashboards can still
-- show "the" court for a case in a single lookup.
CREATE TABLE case_courts (
    case_id     uuid        NOT NULL REFERENCES cases(id)  ON DELETE CASCADE,
    court_id    uuid        NOT NULL REFERENCES courts(id) ON DELETE CASCADE,
    tenant_id   uuid        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    is_primary  boolean     NOT NULL DEFAULT false,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (case_id, court_id)
);

-- Each case has at most one primary court.  Partial unique index is the
-- cleanest way to express that constraint.
CREATE UNIQUE INDEX idx_case_courts_one_primary_per_case
    ON case_courts (case_id) WHERE is_primary;

CREATE INDEX idx_case_courts_tenant_case  ON case_courts (tenant_id, case_id);
CREATE INDEX idx_case_courts_tenant_court ON case_courts (tenant_id, court_id);

ALTER TABLE case_courts ENABLE ROW LEVEL SECURITY;
ALTER TABLE case_courts FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON case_courts
    USING      (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);


-- -----------------------------------------------------------------------------
-- case_citations — `cites` edge (case → case, directed)
-- -----------------------------------------------------------------------------
-- `citation_kind` captures the legal weight: `affirmed`, `distinguished`,
-- `overruled`, `cited`.  S5.6 populates from CourtListener `cites` arrays;
-- without that detail in the source we default to `cited`.  CHECK + closed
-- set keeps callers from inventing new strings on the fly.
CREATE TABLE case_citations (
    citing_case_id  uuid        NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    cited_case_id   uuid        NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    tenant_id       uuid        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    citation_kind   text        NOT NULL DEFAULT 'cited'
        CHECK (citation_kind IN ('cited', 'affirmed', 'distinguished', 'overruled')),
    created_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (citing_case_id, cited_case_id),
    -- A case can't cite itself.
    CONSTRAINT case_citations_no_self_loop CHECK (citing_case_id <> cited_case_id)
);

CREATE INDEX idx_case_citations_tenant_citing ON case_citations (tenant_id, citing_case_id);
CREATE INDEX idx_case_citations_tenant_cited  ON case_citations (tenant_id, cited_case_id);

ALTER TABLE case_citations ENABLE ROW LEVEL SECURITY;
ALTER TABLE case_citations FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON case_citations
    USING      (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);


-- -----------------------------------------------------------------------------
-- Grants — match the existing pattern (jp_app for tenant-scoped reads/writes;
-- jp_admin BYPASSRLS via membership for super-operators).
-- -----------------------------------------------------------------------------
GRANT SELECT, INSERT, UPDATE, DELETE
    ON courts, judges, case_judges, case_courts, case_citations
    TO jp_app;
