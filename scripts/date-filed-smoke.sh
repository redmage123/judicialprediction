#!/bin/bash
# Sprint 11 / S11.6 — date_filed end-to-end smoke.
#
# Verifies:
#   1. createCase with dateFiled=1969-06-15 + MARSHALL opinion text:
#      ideologyProvenance.term == 1969 (asOfYear fed automatically from year(date_filed)).
#      cases.date_filed persisted to the row.
#   2. case(id) round-trip returns the same dateFiled.
#   3. listCases now includes dateFiled in its summary rows.
set -euo pipefail

JP="${JP:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$JP"

WEB_BASE="${WEB_BASE:-http://localhost:3030}"
OPERATOR_EMAIL="${OPERATOR_EMAIL:-dev-tenant1@example.test}"
OPERATOR_PASSWORD="${OPERATOR_PASSWORD:-tenant1-pw}"

echo "==> 1) Authenticate"
COOKIE=$(mktemp); trap 'rm -f "$COOKIE"' EXIT
curl -sS -c "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"email\":\"$OPERATOR_EMAIL\",\"password\":\"$OPERATOR_PASSWORD\"}" \
  "$WEB_BASE/api/auth/login" >/dev/null

echo "==> 2) createCase with dateFiled=1969-06-15 — MQ resolver should auto-pick term 1969"
RESP=$(curl -sS -b "$COOKIE" -X POST -H 'Content-Type: application/json' \
  -d '{"query":"mutation($i:PredictInput!,$o:String,$d:String){createCase(input:$i,opinionText:$o,dateFiled:$d){id dateFiled ideologyProvenance}}","variables":{"i":{"judgeSeverity":0.5,"attorneyWinRate":0.6,"ideologyDistance":0.3,"materialityScore":0.7,"proceduralMotionCount":3,"caseType":"civil","jurisdiction":"us-federal"},"o":"MARSHALL, Judge: The Court reverses the judgment of the court below.","d":"1969-06-15"}}' \
  "$WEB_BASE/api/graphql")
echo "$RESP" | python3 -m json.tool

CASE_ID=$(python3 - "$RESP" <<'PY'
import json, sys
c = json.loads(sys.argv[1])["data"]["createCase"]
if c.get("dateFiled") != "1969-06-15":
    print(f"FAIL: dateFiled={c.get('dateFiled')!r}, want '1969-06-15'", file=sys.stderr)
    sys.exit(1)
p = c.get("ideologyProvenance") or {}
if p.get("term") != 1969:
    print(f"FAIL: ideologyProvenance.term={p.get('term')!r}, want 1969 (auto-fed from year(dateFiled))", file=sys.stderr)
    sys.exit(1)
if p.get("source") != "martin_quinn":
    print(f"FAIL: source={p.get('source')!r}", file=sys.stderr)
    sys.exit(1)
print(c["id"])
PY
)
echo "  case_id=$CASE_ID  date_filed=1969-06-15 -> MQ term=1969"

echo "==> 3) case(id) round-trip: dateFiled preserved"
RESP=$(curl -sS -b "$COOKIE" -X POST -H 'Content-Type: application/json' \
  -d "{\"query\":\"query(\$id:ID!){case(id:\$id){id dateFiled ideologyProvenance}}\",\"variables\":{\"id\":\"$CASE_ID\"}}" \
  "$WEB_BASE/api/graphql")
python3 - "$RESP" <<'PY'
import json, sys
c = json.loads(sys.argv[1])["data"]["case"]
if c.get("dateFiled") != "1969-06-15":
    print(f"FAIL: case(id) lost dateFiled: {c.get('dateFiled')!r}", file=sys.stderr)
    sys.exit(1)
print(f"  round-trip OK: dateFiled={c['dateFiled']}")
PY

echo "==> 4) listCases includes dateFiled in summary rows"
# Use limit:100 because our 1969-filed test case sorts to the bottom of
# the list (older dates sort last under COALESCE(date_filed, created_at)
# DESC) and might be off the first page in noisier dev DBs.
RESP=$(curl -sS -b "$COOKIE" -X POST -H 'Content-Type: application/json' \
  -d '{"query":"{listCases(limit:100){nodes{id dateFiled createdAt}}}"}' \
  "$WEB_BASE/api/graphql")
python3 - "$RESP" <<'PY'
import json, sys
nodes = json.loads(sys.argv[1])["data"]["listCases"]["nodes"]
have_dated = [n for n in nodes if n.get("dateFiled")]
if not have_dated:
    print("FAIL: no dateFiled rows visible in listCases (did Sprint 11 INSERT path fire?)", file=sys.stderr)
    sys.exit(1)
print(f"  listCases has {len(have_dated)}/{len(nodes)} rows with dateFiled — example: {have_dated[0]['dateFiled']}")
PY

echo "==> PASS — date_filed flows end-to-end + MQ resolver auto-fed from filing year"
