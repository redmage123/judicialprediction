#!/bin/bash
set -uo pipefail
COOKIE_JAR=/tmp/jp-final-cookies; rm -f $COOKIE_JAR
PASS=0; FAIL=0
ok() { echo "  PASS: $1"; PASS=$((PASS+1)); }
ko() { echo "  FAIL: $1 — $2"; FAIL=$((FAIL+1)); }

echo "=== 1. login ==="
LOGIN=$(curl -sf -c $COOKIE_JAR -X POST http://127.0.0.1:3030/api/auth/login \
    -H "Content-Type: application/json" \
    -d '{"email":"dev-tenant1@example.test","password":"tenant1-pw"}' \
    -o /dev/null -w "%{http_code}")
[ "$LOGIN" = "200" ] && ok "login HTTP 200" || ko "login" "got $LOGIN"

echo
echo "=== 2. createCase ==="
RES=$(curl -sf -b $COOKIE_JAR -X POST http://127.0.0.1:3030/api/graphql \
    -H "Content-Type: application/json" \
    -d '{"query":"mutation C($input: PredictInput!){ createCase(input: $input) { id prediction { pWin } recommendation { kind } } }","variables":{"input":{"judgeSeverity":0.5,"attorneyWinRate":0.6,"ideologyDistance":0.4,"materialityScore":0.7,"proceduralMotionCount":3,"caseType":"civil","jurisdiction":"us-federal"}}}')
CASE_ID=$(echo "$RES" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["createCase"]["id"])' 2>/dev/null)
echo "  CASE_ID=$CASE_ID"
[ -n "$CASE_ID" ] && ok "createCase returned UUID" || ko "createCase" "no id"

echo
echo "=== 3. /case/[id] page ==="
curl -sf -b $COOKIE_JAR http://127.0.0.1:3030/case/$CASE_ID -o /tmp/p1.html -w "  bytes=%{size_download}\n"
grep -q "Probability\|P(win)" /tmp/p1.html && ok "page has prediction" || ko "page" "no prediction in HTML"
grep -q "Settle\|Try\|Borderline" /tmp/p1.html && ok "page has recommendation badge" || ko "page" "no recommendation badge"

echo
echo "=== 4. /cases list ==="
curl -sf -b $COOKIE_JAR http://127.0.0.1:3030/cases -o /tmp/p2.html -w "  bytes=%{size_download}\n"
grep -q "us-federal\|civil" /tmp/p2.html && ok "list has the case row" || ko "list" "case not visible"
grep -q "Showing\|of " /tmp/p2.html && ok "list has pagination text" || ko "pagination" "no Showing N of M"

echo
echo "=== 5. PDF memo ==="
curl -sf -b $COOKIE_JAR "http://127.0.0.1:3030/api/case/$CASE_ID/memo.pdf" -o /tmp/memo.pdf -w "  HTTP=%{http_code} bytes=%{size_download} ctype=%{content_type}\n"
HEAD=$(head -c 4 /tmp/memo.pdf 2>/dev/null)
[ "$HEAD" = "%PDF" ] && ok "PDF magic bytes" || ko "PDF" "head=$HEAD"
SIZE=$(stat -c %s /tmp/memo.pdf 2>/dev/null || echo 0)
[ "$SIZE" -gt 1024 ] && ok "PDF > 1 KB ($SIZE bytes)" || ko "PDF size" "$SIZE bytes"

echo
echo "=== 6. audit_log ==="
sleep 2
COUNT=$(docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA -c "SELECT COUNT(*) FROM audit_log WHERE action='case.created' AND ts > NOW() - INTERVAL '2 minutes';")
echo "  rows: $COUNT"
[ "$COUNT" -gt 0 ] 2>/dev/null && ok "audit_log row" || ko "audit" "$COUNT rows"

echo
echo "=========================================="
echo "  PASSED: $PASS"
echo "  FAILED: $FAIL"
echo "=========================================="
exit $FAIL
