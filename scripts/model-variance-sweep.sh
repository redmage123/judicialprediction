#!/bin/bash
# Sprint 12 / S12.2 — model variance sweep.
#
# Runs 20 deliberately-varied prediction inputs through the live model
# and reports the pWin distribution.  Used to confirm the Sprint-11
# jurisdiction fix actually unstuck predictions before we retrain.
#
# PASS criterion: max(pWin) - min(pWin) >= 0.10.
#
# Pre-conditions: dev stack up, operator credentials seeded.
set -euo pipefail

JP="${JP:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$JP"

WEB_BASE="${WEB_BASE:-http://localhost:3030}"
OPERATOR_EMAIL="${OPERATOR_EMAIL:-dev-tenant1@example.test}"
OPERATOR_PASSWORD="${OPERATOR_PASSWORD:-tenant1-pw}"

COOKIE=$(mktemp); trap 'rm -f "$COOKIE"' EXIT
curl -sS -c "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"email\":\"$OPERATOR_EMAIL\",\"password\":\"$OPERATOR_PASSWORD\"}" \
  "$WEB_BASE/api/auth/login" >/dev/null

call() {
    local js="$1"
    local query='{"query":"mutation($i:PredictInput!){predictCaseOutcome(input:$i){pWin}}","variables":{"i":'"$js"'}}'
    curl -sS -b "$COOKIE" -X POST -H 'Content-Type: application/json' \
        -d "$query" "$WEB_BASE/api/graphql"
}

inputs=(
    '{"judgeSeverity":0.1,"attorneyWinRate":0.9,"ideologyDistance":0.1,"materialityScore":0.5,"proceduralMotionCount":1,"caseType":"civil","jurisdiction":"us-federal"}'
    '{"judgeSeverity":0.9,"attorneyWinRate":0.1,"ideologyDistance":0.9,"materialityScore":0.5,"proceduralMotionCount":10,"caseType":"civil","jurisdiction":"us-federal"}'
    '{"judgeSeverity":0.5,"attorneyWinRate":0.5,"ideologyDistance":0.5,"materialityScore":0.5,"proceduralMotionCount":5,"caseType":"criminal","jurisdiction":"us-federal"}'
    '{"judgeSeverity":0.2,"attorneyWinRate":0.8,"ideologyDistance":0.2,"materialityScore":0.8,"proceduralMotionCount":2,"caseType":"criminal","jurisdiction":"ca-state"}'
    '{"judgeSeverity":0.8,"attorneyWinRate":0.2,"ideologyDistance":0.8,"materialityScore":0.2,"proceduralMotionCount":8,"caseType":"bankruptcy","jurisdiction":"nj-state"}'
    '{"judgeSeverity":0.3,"attorneyWinRate":0.7,"ideologyDistance":0.4,"materialityScore":0.6,"proceduralMotionCount":3,"caseType":"civil","jurisdiction":"ca-state"}'
    '{"judgeSeverity":0.6,"attorneyWinRate":0.4,"ideologyDistance":0.6,"materialityScore":0.4,"proceduralMotionCount":7,"caseType":"civil","jurisdiction":"nj-state"}'
    '{"judgeSeverity":0.0,"attorneyWinRate":1.0,"ideologyDistance":0.0,"materialityScore":1.0,"proceduralMotionCount":0,"caseType":"civil","jurisdiction":"us-federal"}'
    '{"judgeSeverity":1.0,"attorneyWinRate":0.0,"ideologyDistance":1.0,"materialityScore":0.0,"proceduralMotionCount":15,"caseType":"criminal","jurisdiction":"us-federal"}'
    '{"judgeSeverity":0.4,"attorneyWinRate":0.6,"ideologyDistance":0.3,"materialityScore":0.7,"proceduralMotionCount":4,"caseType":"bankruptcy","jurisdiction":"ca-state"}'
    '{"judgeSeverity":0.7,"attorneyWinRate":0.3,"ideologyDistance":0.7,"materialityScore":0.3,"proceduralMotionCount":6,"caseType":"bankruptcy","jurisdiction":"nj-state"}'
    '{"judgeSeverity":0.25,"attorneyWinRate":0.75,"ideologyDistance":0.25,"materialityScore":0.75,"proceduralMotionCount":2,"caseType":"civil","jurisdiction":"us-federal"}'
    '{"judgeSeverity":0.75,"attorneyWinRate":0.25,"ideologyDistance":0.75,"materialityScore":0.25,"proceduralMotionCount":12,"caseType":"criminal","jurisdiction":"ca-state"}'
    '{"judgeSeverity":0.5,"attorneyWinRate":0.5,"ideologyDistance":0.5,"materialityScore":0.5,"proceduralMotionCount":5,"caseType":"bankruptcy","jurisdiction":"us-federal"}'
    '{"judgeSeverity":0.15,"attorneyWinRate":0.85,"ideologyDistance":0.15,"materialityScore":0.85,"proceduralMotionCount":1,"caseType":"civil","jurisdiction":"nj-state"}'
    '{"judgeSeverity":0.85,"attorneyWinRate":0.15,"ideologyDistance":0.85,"materialityScore":0.15,"proceduralMotionCount":9,"caseType":"criminal","jurisdiction":"nj-state"}'
    '{"judgeSeverity":0.35,"attorneyWinRate":0.65,"ideologyDistance":0.35,"materialityScore":0.65,"proceduralMotionCount":3,"caseType":"civil","jurisdiction":"ca-state"}'
    '{"judgeSeverity":0.65,"attorneyWinRate":0.35,"ideologyDistance":0.65,"materialityScore":0.35,"proceduralMotionCount":7,"caseType":"bankruptcy","jurisdiction":"us-federal"}'
    '{"judgeSeverity":0.45,"attorneyWinRate":0.55,"ideologyDistance":0.45,"materialityScore":0.55,"proceduralMotionCount":4,"caseType":"criminal","jurisdiction":"nj-state"}'
    '{"judgeSeverity":0.55,"attorneyWinRate":0.45,"ideologyDistance":0.55,"materialityScore":0.45,"proceduralMotionCount":6,"caseType":"civil","jurisdiction":"ca-state"}'
)

scores=()
for i in "${inputs[@]}"; do
    resp=$(call "$i")
    p=$(echo "$resp" | python3 -c "import json,sys; print(json.load(sys.stdin)['data']['predictCaseOutcome']['pWin'])" 2>/dev/null || echo "null")
    scores+=("$p")
done

echo "Predictions (pWin):"
printf '  %s\n' "${scores[@]}"

python3 - "${scores[@]}" <<'PY'
import sys
vals = [float(s) for s in sys.argv[1:] if s not in ("null", "")]
if not vals:
    print("FAIL: no predictions returned", file=sys.stderr)
    sys.exit(1)
mn, mx = min(vals), max(vals)
spread = mx - mn
mean = sum(vals) / len(vals)
print(f"\nSummary: n={len(vals)} min={mn:.4f} max={mx:.4f} spread={spread:.4f} mean={mean:.4f}")
if spread < 0.10:
    print(f"FAIL: spread {spread:.4f} < 0.10 — model still effectively flat; retrain required")
    sys.exit(1)
print(f"PASS: spread {spread:.4f} >= 0.10 — model varies across inputs")
PY
