-- =============================================================================
-- JudicialPredict — jp_admin role for Django admin super-operators (S3.9)
--
-- jp_admin: LOGIN + BYPASSRLS.  Super-operators in the Django admin console
-- connect as jp_admin so they can read and write across ALL tenant rows without
-- being subject to the RLS policies that scope jp_app queries to a single tenant.
--
-- Role hierarchy:
--   judicialpredict (superuser, migration-time only)
--       └── jp_admin  (BYPASSRLS LOGIN — Django admin super-operators)
--               └── jp_app  (no BYPASSRLS LOGIN — normal tenant-scoped operators)
--
-- ADR-003: jp_admin bypasses RLS at the Postgres level.  The Django
-- RLSMiddleware (core/middleware.py) handles request-level scoping for
-- role='admin'/'viewer' operators; role='super' operators route to the
-- admin_super DATABASES alias which connects as jp_admin.
--
-- Sprint-4 follow-ups:
--   - Rotate passwords via Vault / Kubernetes secrets.
--   - Replace static password with certificate-based auth (sslmode=verify-full).
-- =============================================================================

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'jp_admin') THEN
        CREATE ROLE jp_admin LOGIN BYPASSRLS PASSWORD 'judicialpredict_admin_pwd'
            NOSUPERUSER NOCREATEDB NOCREATEROLE;
    END IF;
END$$;

-- Schema access.
GRANT USAGE ON SCHEMA public TO jp_admin;

-- Full DML on all application tables (BYPASSRLS means RLS policies are ignored).
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE tenants          TO jp_admin;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE cases            TO jp_admin;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE features         TO jp_admin;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE audit_log        TO jp_admin;
GRANT USAGE ON SEQUENCE audit_log_id_seq                       TO jp_admin;
GRANT SELECT                          ON TABLE _sqlx_migrations TO jp_admin;

-- Enum types.
GRANT USAGE ON TYPE feature_tier        TO jp_admin;
GRANT USAGE ON TYPE feature_sensitivity TO jp_admin;

-- tenant_settings (S2.12)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables
               WHERE table_schema = 'public' AND table_name = 'tenant_settings') THEN
        EXECUTE 'GRANT SELECT, INSERT, UPDATE ON TABLE tenant_settings TO jp_admin';
    END IF;
END$$;

-- Django-managed tables created by python/admin manage.py migrate.
-- Grants are applied after Django creates the tables; idempotent.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables
               WHERE table_schema = 'public' AND table_name = 'operators_operator') THEN
        EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE operators_operator TO jp_admin';
    END IF;
END$$;

-- Default privileges: future tables created by migrations are also accessible.
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO jp_admin;
