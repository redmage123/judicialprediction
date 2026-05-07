-- =============================================================================
-- JudicialPredict — Baseline schema migration
-- ADR-003: Postgres + RLS enforcing tenant isolation
-- ADR-004: feature_tier / feature_sensitivity enum types match proto enums
-- =============================================================================

-- -----------------------------------------------------------------------------
-- Extensions
-- -----------------------------------------------------------------------------
CREATE EXTENSION IF NOT EXISTS pgcrypto;   -- gen_random_uuid()
CREATE EXTENSION IF NOT EXISTS vector;     -- pgvector for future embedding columns

-- -----------------------------------------------------------------------------
-- Enum types
-- NOTE: SQL enums cannot carry a zero-sentinel, so TIER_UNSPECIFIED / UNSPECIFIED
-- are intentionally absent. Application code must reject those proto values
-- before persisting; see ADR-004.
-- -----------------------------------------------------------------------------
CREATE TYPE feature_tier AS ENUM (
    'TIER_A',
    'TIER_B',
    'TIER_C',
    'TIER_D'    -- reserved for future regulatory expansion
);

CREATE TYPE feature_sensitivity AS ENUM (
    'PUBLIC',
    'QUASI_PUBLIC',
    'INFERRED',
    'PROTECTED'
);

-- -----------------------------------------------------------------------------
-- tenants — one row per client organisation
-- RLS: each tenant can only see its own row.
-- -----------------------------------------------------------------------------
CREATE TABLE tenants (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    slug        text        UNIQUE NOT NULL,
    name        text        NOT NULL,
    settings    jsonb       NOT NULL DEFAULT '{}',
    created_at  timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE tenants ENABLE ROW LEVEL SECURITY;
-- FORCE applies RLS to the table owner as well (migration role = owner).
ALTER TABLE tenants FORCE ROW LEVEL SECURITY;

-- Tenants can only see themselves.
CREATE POLICY tenant_isolation ON tenants
    USING      (id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (id = current_setting('app.current_tenant_id', true)::uuid);

-- -----------------------------------------------------------------------------
-- updated_at trigger function (reused across tables)
-- -----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

-- -----------------------------------------------------------------------------
-- cases — one legal case per row
-- -----------------------------------------------------------------------------
CREATE TABLE cases (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    title       text        NOT NULL,
    jurisdiction text       NOT NULL,
    court       text,
    judge_name  text,
    parties     jsonb       NOT NULL DEFAULT '{}',
    claims      jsonb       NOT NULL DEFAULT '[]',
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_cases_tenant_id       ON cases (tenant_id);
CREATE INDEX idx_cases_tenant_judge    ON cases (tenant_id, judge_name);

CREATE TRIGGER trg_cases_updated_at
    BEFORE UPDATE ON cases
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

ALTER TABLE cases ENABLE ROW LEVEL SECURITY;
ALTER TABLE cases FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON cases
    USING      (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

-- -----------------------------------------------------------------------------
-- features — typed, tier-classified feature values per case
-- -----------------------------------------------------------------------------
CREATE TABLE features (
    id          uuid                PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid                NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    case_id     uuid                REFERENCES cases(id) ON DELETE CASCADE,
    name        text                NOT NULL,
    value       jsonb               NOT NULL,
    tier        feature_tier        NOT NULL,
    sensitivity feature_sensitivity NOT NULL,
    source      text                NOT NULL,
    lineage     jsonb               NOT NULL DEFAULT '{}',
    created_at  timestamptz         NOT NULL DEFAULT now()
);

CREATE INDEX idx_features_tenant_id       ON features (tenant_id);
CREATE INDEX idx_features_tenant_case     ON features (tenant_id, case_id);

ALTER TABLE features ENABLE ROW LEVEL SECURITY;
ALTER TABLE features FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON features
    USING      (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

-- -----------------------------------------------------------------------------
-- audit_log — append-only; no RLS (service-layer writes only; DBA reads)
-- tenant_id nullable so superuser/migration events can be logged without a tenant.
-- -----------------------------------------------------------------------------
CREATE TABLE audit_log (
    id          bigserial   PRIMARY KEY,
    tenant_id   uuid,                    -- nullable: allows system-level events
    subject_id  text,                    -- authenticated user / service principal
    table_name  text        NOT NULL,
    row_pk      text,                    -- stringified PK of affected row
    action      text        NOT NULL,    -- INSERT | UPDATE | DELETE | SELECT_TIER_C
    reason_code text,                    -- PermittedUse value for Tier-C reads
    ts          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_log_tenant_ts ON audit_log (tenant_id, ts DESC);
