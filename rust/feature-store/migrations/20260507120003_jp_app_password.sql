-- =============================================================================
-- JudicialPredict — Give jp_app a login password for direct connections.
-- S1.11: The application pool connects as jp_app (non-superuser) so RLS is
-- enforced at query time (jp_app has no BYPASSRLS). Migration runs as the
-- superuser; runtime queries run as jp_app.
-- =============================================================================

-- Idempotent: ALTER ROLE is safe to re-run.
ALTER ROLE jp_app LOGIN PASSWORD 'judicialpredict_dev_pwd';
