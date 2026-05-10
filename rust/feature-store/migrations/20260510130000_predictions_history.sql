-- =============================================================================
-- JudicialPredict — S4.7 (JP-61): Prediction history table + cases.updated_at
--
-- Design decisions:
--   * predictions is insert-only — rows are never UPDATEd or DELETEd (only
--     cascade-deleted when the parent case is removed).
--   * cases.prediction always reflects the latest prediction; the full
--     ordered history is in the predictions table.
--   * cases.updated_at is NULL for rows created before S4.7; repredictCase
--     sets it to now() on every re-run.
--   * RLS mirrors the cases policies: tenant isolation via SET LOCAL
--     app.current_tenant_id + explicit WHERE tenant_id = $n.
-- =============================================================================

-- -----------------------------------------------------------------------------
-- 1. Add updated_at to cases (nullable; pre-S4.7 rows stay NULL)
-- -----------------------------------------------------------------------------
ALTER TABLE cases
    ADD COLUMN IF NOT EXISTS updated_at timestamptz;

COMMENT ON COLUMN cases.updated_at IS
    'Set to now() by repredictCase (S4.7) on every re-prediction run. NULL for cases
created before S4.7.';

-- -----------------------------------------------------------------------------
-- 2. Prediction history table
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS predictions (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id         uuid        NOT NULL REFERENCES cases(id)   ON DELETE CASCADE,
    tenant_id       uuid        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    prediction      jsonb       NOT NULL,
    model_version   text        NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS predictions_case_created_idx
    ON predictions (case_id, created_at DESC);

COMMENT ON TABLE predictions IS
    'Per-case prediction history. cases.prediction always points at the latest row here.
Insert-only; never updated. Cascade-deletes when the parent case is removed.';

-- -----------------------------------------------------------------------------
-- 3. Row-Level Security on predictions
-- -----------------------------------------------------------------------------
ALTER TABLE predictions ENABLE ROW LEVEL SECURITY;
ALTER TABLE predictions FORCE ROW LEVEL SECURITY;

CREATE POLICY predictions_tenant_select ON predictions FOR SELECT
    USING (tenant_id::text = current_setting('app.current_tenant_id', true));

CREATE POLICY predictions_tenant_modify ON predictions FOR ALL
    USING (tenant_id::text = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));

-- -----------------------------------------------------------------------------
-- 4. Privileges
-- -----------------------------------------------------------------------------
-- jp_app: SELECT for casePredictions query, INSERT for repredictCase.
-- No UPDATE or DELETE — predictions are insert-only by design.
GRANT SELECT, INSERT ON predictions TO jp_app;

-- jp_admin (BYPASSRLS) gets full access so integration tests can seed/clean rows.
GRANT ALL ON predictions TO jp_admin;
