#!/bin/bash
# End-to-end smoke: login → predict → audit verification
set -uo pipefail

WEB=http://127.0.0.1:3030
GW=http://127.0.0.1:4040
ML=http://127.0.0.1:8001

PASS=0
FAIL=0
ok() { echo "  PASS: $1"; PASS=$((PASS+1)); }
ko() { echo "  FAIL: $1 — $2"; FAIL=$((FAIL+1)); }

# ----------------------------------------------------------------------------
echo "=== 1. /healthz on all three services ==="
for url in "$WEB/login" "$GW/health" "$ML/healthz"; do
    code=$(curl -sf -o /dev/null -w "%{http_code}" "$url" || echo 000)
    [ "$code" = "200" ] && ok "$url → 200" || ko "$url" "got $code"
done

# ----------------------------------------------------------------------------
echo
echo "=== 2. dev login mints a session cookie ==="
COOKIE_JAR=/tmp/jp-smoke-cookies
rm -f $COOKIE_JAR
LOGIN_RES=$(curl -sf -c $COOKIE_JAR -X POST "$WEB/api/auth/login" \
    -H "Content-Type: application/json" \
    -d '{"email":"dev@example.test","password":"dev-pass"}' \
    -w "\nHTTP=%{http_code}")
echo "$LOGIN_RES"
echo "$LOGIN_RES" | grep -q "HTTP=200" && ok "login HTTP 200" || ko "login" "non-200"
grep -q "jp_session" $COOKIE_JAR && ok "jp_session cookie set" || ko "cookie" "no jp_session in jar"

# ----------------------------------------------------------------------------
echo
echo "=== 3. unauth /case/new redirects to /login ==="
RES=$(curl -s -o /dev/null -w "%{http_code} %{redirect_url}" "$WEB/case/new")
echo "  raw: $RES"
echo "$RES" | grep -q "^307\|^302\|^308" && ok "unauth redirect" || ko "unauth redirect" "got $RES"

# ----------------------------------------------------------------------------
echo
echo "=== 4. authenticated /case/new returns 200 ==="
code=$(curl -sf -b $COOKIE_JAR -o /dev/null -w "%{http_code}" "$WEB/case/new")
[ "$code" = "200" ] && ok "/case/new authenticated → 200" || ko "/case/new auth" "got $code"

# ----------------------------------------------------------------------------
echo
echo "=== 5. predictCaseOutcome via /api/graphql proxy ==="
GQL_QUERY='{"query":"mutation P($input: PredictInput!){ predictCaseOutcome(input: $input) { pWin ciLower ciUpper coverage modelVersion predictedAtUnix } }","variables":{"input":{"judgeSeverity":0.65,"attorneyWinRate":0.72,"ideologyDistance":0.41,"materialityScore":0.88,"proceduralMotionCount":3,"caseType":"civil","jurisdiction":"us-federal"}}}'
GQL_RES=$(curl -s -b $COOKIE_JAR -X POST "$WEB/api/graphql" \
    -H "Content-Type: application/json" \
    -d "$GQL_QUERY")
echo "  response: $GQL_RES" | head -c 400
echo
echo "$GQL_RES" | grep -q "predictCaseOutcome" && ok "graphql data path" || ko "graphql" "no predictCaseOutcome in body"
echo "$GQL_RES" | grep -q '"pWin"' && ok "pWin in response" || ko "pWin" "missing"

# ----------------------------------------------------------------------------
echo
echo "=== 6. audit_log row was written for the predict call ==="
sleep 2  # fire-and-forget audit
AUDIT_COUNT=$(docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA \
    -c "SELECT COUNT(*) FROM audit_log WHERE action='predict.invoke' AND ts > NOW() - INTERVAL '1 minute';")
echo "  recent predict.invoke rows: $AUDIT_COUNT"
[ "$AUDIT_COUNT" -gt 0 ] 2>/dev/null && ok "audit_log row present" || ko "audit" "$AUDIT_COUNT predict.invoke rows in last 1m"

# ----------------------------------------------------------------------------
echo
echo "=========================================="
echo "  PASSED: $PASS"
echo "  FAILED: $FAIL"
echo "=========================================="
exit $FAIL
