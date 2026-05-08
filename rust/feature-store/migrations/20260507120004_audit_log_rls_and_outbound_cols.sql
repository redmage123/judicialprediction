-- =============================================================================
-- JudicialPredict — Extend audit_log for proxy-audit MVP (S2.11, JP-34)
-- * Add latency_ms + cost_micros columns for outbound-call audit events.
-- * Enable RLS so each tenant can read only its own audit rows.
-- * Grant SELECT on audit_log to jp_app (was INSERT-only).
-- =============================================================================

-- ── New columns ──────────────────────────────────────────────────────────────
-- Both nullable: latency is always set for outbound calls; cost is optional
-- (LLM tokens, third-party API credits, etc.).
ALTER TABLE audit_log
    ADD COLUMN IF NOT EXISTS latency_ms  integer,
    ADD COLUMN IF NOT EXISTS cost_micros integer;

-- ── Row-level security ───────────────────────────────────────────────────────
-- Note: the migration role (judicialpredict) has BYPASSRLS so existing migration
-- inserts are unaffected.  jp_app has no BYPASSRLS, so policies are evaluated
-- for every query it runs.

ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_log FORCE ROW LEVEL SECURITY;

-- SELECT: a tenant sees only rows where tenant_id matches the current context.
-- Rows with NULL tenant_id (system-level events) are intentionally invisible
-- to all tenant contexts — DBA-only via the superuser role.
CREATE POLICY audit_log_select ON audit_log
    FOR SELECT
    USING (
        tenant_id IS NOT NULL
        AND tenant_id = current_setting('app.current_tenant_id', true)::uuid
    );

-- INSERT: a tenant context may only insert rows for itself.
-- NULL tenant_id is allowed for system/migration events (called by the
-- superuser which bypasses this policy anyway).
CREATE POLICY audit_log_insert ON audit_log
    FOR INSERT
    WITH CHECK (
        tenant_id IS NULL
        OR tenant_id = current_setting('app.current_tenant_id', true)::uuid
    );

-- ── Grant SELECT to jp_app ────────────────────────────────────────────────────
-- Previously jp_app could only INSERT; it now needs SELECT for the
-- cross-plane RLS integration test and for future tenant audit dashboards.
GRANT SELECT ON TABLE audit_log TO jp_app;
