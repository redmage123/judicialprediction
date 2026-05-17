-- Sprint 11 / S11.2 — operator-supplied filing date on cases.
--
-- Lets the intake form record the actual date a matter was filed so:
--   1. The dashboard's "DATE FILED" column shows something meaningful
--      (was always created_at, which is just when we ran the prediction).
--   2. The date-aware MQ resolver (Sprint 10) fires automatically — the
--      gateway derives as_of_year from this column and feeds it into
--      extract_features_from_text, picking the MQ term that was current
--      when the case was filed.
--
-- Nullable: legacy cases stay NULL.  Dashboard falls back to created_at
-- for display.

BEGIN;

ALTER TABLE cases
  ADD COLUMN IF NOT EXISTS date_filed DATE;

COMMENT ON COLUMN cases.date_filed IS
$col$
Operator-supplied filing date for the case.  When non-NULL, the gateway
derives `year(date_filed)` for the MQ as-of-year lookup so historical
cases pull the appropriate term snapshot.  When NULL (legacy cases or
operator omitted the field), the resolver uses the MQ latest snapshot
and the dashboard displays `created_at` instead.
$col$;

-- Partial index for "recent by filing date" sorts on the dashboard.
-- COALESCE(date_filed, created_at) doesn't index well, so we index just
-- the populated column and let the query planner handle the COALESCE
-- via a secondary sort.
CREATE INDEX IF NOT EXISTS cases_tenant_date_filed_idx
    ON cases (tenant_id, date_filed DESC)
    WHERE date_filed IS NOT NULL;

COMMIT;
