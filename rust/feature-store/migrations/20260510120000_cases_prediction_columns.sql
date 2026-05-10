-- =============================================================================
-- JudicialPredict — S4.1 (JP-55): Extend cases with prediction/recommendation
--
-- Adds four nullable jsonb/uuid columns needed for Sprint-4 prediction
-- persistence (S4.2 createCase mutation, S4.5 /cases list page).
--
-- Schema design decisions:
--   * All new columns are nullable so the four existing dev rows remain valid.
--   * No CHECK constraints on jsonb shape — shape validation is the job of the
--     api-gateway resolver (S4.2).  See spec §5.4 follow-up.
--   * created_by is NOT a foreign key because operators live in a Django-managed
--     schema scope; referential integrity is enforced at the API layer.
--
-- Sprint-4 follow-ups (out of scope here):
--   * Cost-engine integration (Sprint 5) — settlement-anchor value will come
--     from a real BATNA model, not the 0.40 placeholder in decision-arith.
--   * jsonb shape constraints, if compliance review requires them (spec §5.4).
-- =============================================================================

-- -----------------------------------------------------------------------------
-- 1. New columns
-- -----------------------------------------------------------------------------
ALTER TABLE cases
    ADD COLUMN IF NOT EXISTS input_features  jsonb,
    ADD COLUMN IF NOT EXISTS prediction      jsonb,
    ADD COLUMN IF NOT EXISTS recommendation  jsonb,
    ADD COLUMN IF NOT EXISTS created_by      uuid;

COMMENT ON COLUMN cases.input_features IS
    'PredictInput JSON: 7 Tier-A/B feature fields (judge_severity, attorney_win_rate, ideology_distance, materiality_score, procedural_motion_count, case_type, jurisdiction). Source of truth for re-running predictions.';
COMMENT ON COLUMN cases.prediction IS
    'PredictResult JSON: {p_win, ci_lower, ci_upper, coverage, model_version, predicted_at_unix}. Written by S4.2 createCase.';
COMMENT ON COLUMN cases.recommendation IS
    'Recommendation JSON: {kind, rationale_bullets[3], expected_value_try, expected_value_settle}. Mirrors decision-arith::Recommendation. Sprint-3 wave-3 follow-up: generated server-side once the TypeScript port in web/lib/recommend.ts is replaced by a recommend GraphQL query.';
COMMENT ON COLUMN cases.created_by IS
    'operators.id of the operator who submitted this case. NOT an enforced FK: operators is in a different schema scope (Django-managed); validation happens at the API layer (S4.2).';

-- -----------------------------------------------------------------------------
-- 2. Composite index for the /cases list page (S4.5) — most recent first
--    per tenant.  EXISTS guard lets the migration be re-run safely.
-- -----------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS cases_tenant_created_idx ON cases (tenant_id, created_at DESC);

-- -----------------------------------------------------------------------------
-- 3. RLS verification — policies set in 20260507120000_baseline.sql must
--    still be in force after this ALTER TABLE.  Assert here so a future
--    accidental DROP POLICY produces an obvious migration failure.
-- -----------------------------------------------------------------------------
DO $$
DECLARE
    rls_enabled   bool;
    rls_forced    bool;
    policy_count  int;
BEGIN
    SELECT relrowsecurity, relforcerowsecurity
      INTO rls_enabled, rls_forced
      FROM pg_class
     WHERE relname = 'cases' AND relnamespace = 'public'::regnamespace;

    IF NOT rls_enabled THEN
        RAISE EXCEPTION 'POLICY REGRESSION: cases.relrowsecurity is FALSE — RLS was disabled.';
    END IF;
    IF NOT rls_forced THEN
        RAISE EXCEPTION 'POLICY REGRESSION: cases.relforcerowsecurity is FALSE — FORCE RLS was disabled.';
    END IF;

    SELECT COUNT(*) INTO policy_count
      FROM pg_policy p
      JOIN pg_class  c ON c.oid = p.polrelid
     WHERE c.relname = 'cases' AND c.relnamespace = 'public'::regnamespace;

    IF policy_count < 1 THEN
        RAISE EXCEPTION 'POLICY REGRESSION: no RLS policies found on cases table.';
    END IF;

    RAISE NOTICE 'RLS check OK: cases has % polic(ies), rowsecurity=%, forcesecurity=%.',
        policy_count, rls_enabled, rls_forced;
END$$;

-- -----------------------------------------------------------------------------
-- 4. jp_app privileges — already granted SELECT/INSERT/UPDATE/DELETE in
--    20260507120002_app_role.sql; these are a belt-and-suspenders re-grant
--    so S4.2 never fails if someone ran with a stripped test role.
-- -----------------------------------------------------------------------------
GRANT SELECT, INSERT, UPDATE ON TABLE cases TO jp_app;

-- jp_admin (BYPASSRLS) already has full DML via 20260509140000_jp_admin_role.sql.
-- No additional grant needed; the DEFAULT PRIVILEGES clause there covers new
-- columns automatically.
