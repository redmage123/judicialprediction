-- =============================================================================
-- JudicialPredict — Dev seed: deterministic tenant for local dev + CI tests
-- ID is fixed so test fixtures can reference it without a lookup.
-- MUST NOT be applied in production (use sqlx migrate run --target-version
-- 20260507120000 to stop before this file in prod).
-- =============================================================================

-- Bypass RLS for this seed INSERT (superuser / migration role has BYPASSRLS).
INSERT INTO tenants (id, slug, name, settings)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    'dev-tenant',
    'Development Tenant',
    '{}'
)
ON CONFLICT (id) DO NOTHING;
