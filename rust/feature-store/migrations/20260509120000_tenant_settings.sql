-- =============================================================================
-- JudicialPredict — Tenant-scoped feature-tier override store (S2.12, JP-35)
-- =============================================================================
--
-- tenant_settings holds per-tenant override configuration for the feature-store.
-- One row per tenant (UNIQUE constraint).  The gRPC GetFeature / ListFeatures
-- handlers consult this table before returning features.
--
-- jsonb shape stored in feature_tier_overrides:
-- {
--   "disabled_features": ["attorney_personality_score", "judge_age_years"],
--   "tier_overrides":    {"attorney_temperament": "TIER_C"}
-- }
--
-- Semantics:
--   disabled_features — feature names in this list are refused with gRPC
--                       PERMISSION_DENIED regardless of the global tier policy.
--   tier_overrides    — key: stable feature name, value: "TIER_A"|"TIER_B"|"TIER_C".
--                       Only TIGHTENING is permitted: a tenant may downgrade a
--                       feature to Tier-C (refuse), but cannot grant a feature
--                       that the global policy forbids.
--
-- Sprint-3 follow-up: Django admin UI for this table lives on JP-38.
-- =============================================================================

CREATE TABLE tenant_settings (
    id                     uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id              uuid        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE UNIQUE,
    feature_tier_overrides jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at             timestamptz NOT NULL DEFAULT now(),
    updated_at             timestamptz NOT NULL DEFAULT now()
);

-- Reuse the set_updated_at() trigger already defined in the baseline migration.
CREATE TRIGGER trg_tenant_settings_updated_at
    BEFORE UPDATE ON tenant_settings
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Index for fast per-tenant lookups.
CREATE INDEX idx_tenant_settings_tenant_id ON tenant_settings (tenant_id);

-- ── Row-level security ───────────────────────────────────────────────────────
-- jp_app has no BYPASSRLS, so all three policies below are enforced at query time.

ALTER TABLE tenant_settings ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_settings FORCE ROW LEVEL SECURITY;

-- SELECT: only visible within the matching tenant context.
CREATE POLICY tenant_settings_select ON tenant_settings
    FOR SELECT
    USING (tenant_id::text = current_setting('app.current_tenant_id', true));

-- INSERT: the first update_overrides call creates the row.
CREATE POLICY tenant_settings_insert ON tenant_settings
    FOR INSERT
    WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));

-- UPDATE: only the owning tenant may update its own settings.
CREATE POLICY tenant_settings_update ON tenant_settings
    FOR UPDATE
    USING      (tenant_id::text = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));

-- ── Grants ───────────────────────────────────────────────────────────────────
GRANT SELECT, INSERT, UPDATE ON tenant_settings TO jp_app;
