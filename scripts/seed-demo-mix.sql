-- ---------------------------------------------------------------------------
-- seed-demo-mix.sql
--
-- Seeds 12 hand-crafted cases against the dev tenant so the /cases dashboard
-- shows a real recommendation mix (Settle / Try / Borderline) rather than the
-- monotone Settle-only output that the current ML model produces from uniform
-- smoke inputs (ci_lower is consistently near 0 in dev → always Settle).
--
-- Each row's prediction + recommendation JSON is consistent with the
-- decision-arith rules (expected_damages = $100k, cost = $50k, settle anchor
-- = 0.40):
--   Settle:     EV_settle > EV_try AND ci_lower < 0.40
--   Try:        EV_try > EV_settle AND ci_lower > 0.55
--   Borderline: anything else
--
-- Usage (from a dev shell that can reach the postgres container):
--   docker exec -i judicialpredict_postgres psql -U judicialpredict \
--     -d judicialpredict_dev < scripts/seed-demo-mix.sql
--
-- Idempotency: this script appends 12 rows on each run.  It does not clear or
-- de-duplicate.  If you need a clean demo state, truncate first:
--   TRUNCATE cases CASCADE;
-- ---------------------------------------------------------------------------

SET app.current_tenant_id = '00000000-0000-0000-0000-000000000001';

WITH seed(case_type, jurisdiction, p_win, ci_lower, ci_upper, kind, ev_try, ev_settle) AS (
    VALUES
    -- Try (high P(win), high ci_lower)
    ('contract',   'us-federal', 0.94, 0.58, 0.99, 'Try',        '44000.00', '40000.00'),
    ('antitrust',  'us-federal', 0.96, 0.62, 0.99, 'Try',        '46000.00', '40000.00'),
    ('patent',     'us-federal', 0.92, 0.60, 0.98, 'Try',        '42000.00', '40000.00'),
    ('torts',      'us-state',   0.95, 0.59, 0.99, 'Try',        '45000.00', '40000.00'),
    ('contract',   'us-state',   0.97, 0.65, 0.99, 'Try',        '47000.00', '40000.00'),
    -- Borderline (mid CI band)
    ('civil',      'us-federal', 0.78, 0.45, 0.92, 'Borderline', '28000.00', '40000.00'),
    ('regulatory', 'us-federal', 0.71, 0.48, 0.88, 'Borderline', '21000.00', '40000.00'),
    ('employment', 'us-state',   0.74, 0.42, 0.90, 'Borderline', '24000.00', '40000.00'),
    ('contract',   'us-state',   0.69, 0.46, 0.86, 'Borderline', '19000.00', '40000.00'),
    -- Settle (low CI lower)
    ('criminal',   'us-state',   0.55, 0.10, 0.85, 'Settle',     '5000.00',  '40000.00'),
    ('torts',      'us-state',   0.42, 0.05, 0.78, 'Settle',     '-8000.00', '40000.00'),
    ('regulatory', 'us-federal', 0.62, 0.18, 0.88, 'Settle',     '12000.00', '40000.00')
)
INSERT INTO cases (
    tenant_id, title, jurisdiction, input_features, prediction, recommendation
)
SELECT
    '00000000-0000-0000-0000-000000000001'::uuid,
    'Demo: ' || initcap(case_type) || ' — ' || jurisdiction,
    jurisdiction,
    jsonb_build_object(
        'case_type', case_type,
        'jurisdiction', jurisdiction,
        'judge_severity', round((random()*0.6 + 0.2)::numeric, 2),
        'attorney_win_rate', round((random()*0.5 + 0.4)::numeric, 2),
        'ideology_distance', round((random()*0.8)::numeric, 2),
        'materiality_score', round((random()*0.6 + 0.3)::numeric, 2),
        'procedural_motion_count', floor(random()*8 + 1)::int
    ),
    jsonb_build_object(
        'p_win',             p_win,
        'ci_lower',          ci_lower,
        'ci_upper',          ci_upper,
        'coverage',          0.9,
        'model_version',     'demo-mix-2026-05-12',
        'predicted_at_unix', extract(epoch from now())::bigint
    ),
    jsonb_build_object(
        'kind',                  kind,
        'expected_value_try',    ev_try,
        'expected_value_settle', ev_settle,
        'rationale_bullets', jsonb_build_array(
            'P(win) ' || to_char(p_win, 'FM0.00')
                || ' with 90% CI [' || to_char(ci_lower, 'FM0.00')
                || ', ' || to_char(ci_upper, 'FM0.00') || ']',
            'Expected value at trial $' || ev_try
                || ' vs. expected settlement value $' || ev_settle,
            CASE kind
              WHEN 'Try' THEN
                'Trial preferred: CI lower bound (' || to_char(ci_lower, 'FM0.00')
                || ') clears the high-confidence threshold of 0.55 and trial EV ($'
                || ev_try || ') exceeds settlement EV ($' || ev_settle || ')'
              WHEN 'Settle' THEN
                'Settlement preferred: CI lower bound (' || to_char(ci_lower, 'FM0.00')
                || ') is below the loss-exposure threshold of 0.40 and settlement EV ($'
                || ev_settle || ') exceeds trial EV ($' || ev_try || ')'
              ELSE
                'Borderline: CI lower bound is in the middle band — neither decisive '
                || 'trial confidence (>0.55) nor decisive settlement exposure (<0.40)'
            END
        )
    )
FROM seed;
