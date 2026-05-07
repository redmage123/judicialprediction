-- =============================================================================
-- JudicialPredict — Application role (non-superuser, subject to RLS)
-- ADR-003: the runtime application connects as jp_app, not as the migration
-- superuser, so tenant_isolation policies are enforced at query time.
-- =============================================================================

-- Create the application role if it does not already exist.
-- jp_app: no login (connection pool will SET ROLE jp_app after connecting
-- as the migration user), no superuser, no BYPASSRLS.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'jp_app') THEN
        CREATE ROLE jp_app NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
    END IF;
END$$;

-- Grant schema usage.
GRANT USAGE ON SCHEMA public TO jp_app;

-- Grant DML on runtime tables (RLS restricts to current_tenant_id at query time).
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE tenants  TO jp_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE cases    TO jp_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE features TO jp_app;
GRANT INSERT                          ON TABLE audit_log TO jp_app;
GRANT USAGE ON SEQUENCE audit_log_id_seq TO jp_app;

-- Grant SELECT on _sqlx_migrations so health-check queries work.
GRANT SELECT ON TABLE _sqlx_migrations TO jp_app;

-- Allow jp_app to use the enum types.
GRANT USAGE ON TYPE feature_tier        TO jp_app;
GRANT USAGE ON TYPE feature_sensitivity TO jp_app;
