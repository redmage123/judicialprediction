#!/bin/bash
# Sprint 10 / S10.6 — per-case provenance + date-aware MQ end-to-end smoke.
#
# Verifies:
#   1. extractFeatures with asOfYear=1969 returns Marshall's 1969 score
#      (not the 1972 latest). Date-aware MQ working.
#   2. createCase with opinion text persists ideology_provenance.
#   3. case(id) returns the provenance snapshot we just wrote.
#
# Pre-conditions: dev stack up, MARSHALL judge seeded with full bio.mqs.scores[]
# (the mqs-smoke.sh script does this).
set -euo pipefail

JP="${JP:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$JP"

TENANT_ID="${TENANT_ID:-00000000-0000-0000-0000-000000000001}"
WEB_BASE="${WEB_BASE:-http://localhost:3030}"
OPERATOR_EMAIL="${OPERATOR_EMAIL:-dev-tenant1@example.test}"
OPERATOR_PASSWORD="${OPERATOR_PASSWORD:-tenant1-pw}"

echo "==> 1) Authenticate"
COOKIE=$(mktemp); trap 'rm -f "$COOKIE"' EXIT
curl -sS -c "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"email\":\"$OPERATOR_EMAIL\",\"password\":\"$OPERATOR_PASSWORD\"}" \
  "$WEB_BASE/api/auth/login" >/dev/null

echo "==> 2) Date-aware MQ: asOfYear=1969 should pick the 1969 term"
RESP=$(curl -sS -b "$COOKIE" -X POST -H 'Content-Type: application/json' \
  -d '{"query":"query($t:String!,$y:Int){extractFeatures(text:$t,asOfYear:$y){judgeName ideologySource ideologyTerm ideologyCfscore}}","variables":{"t":"MARSHALL, Judge: The Court reverses.","y":1969}}' \
  "$WEB_BASE/api/graphql")
echo "$RESP" | python3 -m json.tool
python3 - "$RESP" <<'PY'
import json, sys
ef = json.loads(sys.argv[1])["data"]["extractFeatures"]
if ef.get("ideologyTerm") != 1969:
    print(f"FAIL: ideologyTerm={ef.get('ideologyTerm')!r}, want 1969 (date-aware)")
    sys.exit(1)
print(f"  asOfYear=1969 -> term={ef['ideologyTerm']} score={ef['ideologyCfscore']}")
PY

echo "==> 3) asOfYear omitted: should pick the latest term (1972)"
RESP=$(curl -sS -b "$COOKIE" -X POST -H 'Content-Type: application/json' \
  -d '{"query":"query($t:String!){extractFeatures(text:$t){ideologyTerm ideologyCfscore}}","variables":{"t":"MARSHALL, Judge: The Court reverses."}}' \
  "$WEB_BASE/api/graphql")
python3 - "$RESP" <<'PY'
import json, sys
ef = json.loads(sys.argv[1])["data"]["extractFeatures"]
if ef.get("ideologyTerm") != 1972:
    print(f"FAIL: ideologyTerm={ef.get('ideologyTerm')!r}, want 1972 (latest snapshot)")
    sys.exit(1)
print(f"  latest -> term={ef['ideologyTerm']} score={ef['ideologyCfscore']}")
PY

echo "==> 4) createCase with opinion text should persist ideology_provenance"
RESP=$(curl -sS -b "$COOKIE" -X POST -H 'Content-Type: application/json' \
  -d '{"query":"mutation($i:PredictInput!,$o:String){createCase(input:$i,opinionText:$o){id ideologyProvenance recommendation{kind}}}","variables":{"i":{"judgeSeverity":0.5,"attorneyWinRate":0.6,"ideologyDistance":0.3,"materialityScore":0.7,"proceduralMotionCount":3,"caseType":"civil","jurisdiction":"us-federal"},"o":"MARSHALL, Judge: The Court reverses the judgment of the court below."}}' \
  "$WEB_BASE/api/graphql")
echo "$RESP" | python3 -m json.tool
CASE_ID=$(python3 - "$RESP" <<'PY'
import json, sys
c = json.loads(sys.argv[1])["data"]["createCase"]
p = c["ideologyProvenance"]
if p is None:
    print("FAIL: ideologyProvenance is null on createCase", file=sys.stderr)
    sys.exit(1)
if p["source"] != "martin_quinn":
    print(f"FAIL: source={p['source']!r}, want 'martin_quinn'", file=sys.stderr)
    sys.exit(1)
if "release" not in p or "resolved_at" not in p:
    print(f"FAIL: provenance missing release / resolved_at", file=sys.stderr)
    sys.exit(1)
print(c["id"])
PY
)
echo "  case_id=$CASE_ID  provenance.source=martin_quinn"

echo "==> 5) case(id) round-trip should return the snapshot we just wrote"
RESP=$(curl -sS -b "$COOKIE" -X POST -H 'Content-Type: application/json' \
  -d "{\"query\":\"query(\$id:ID!){case(id:\$id){id ideologyProvenance}}\",\"variables\":{\"id\":\"$CASE_ID\"}}" \
  "$WEB_BASE/api/graphql")
python3 - "$RESP" <<'PY'
import json, sys
c = json.loads(sys.argv[1])["data"]["case"]
p = c["ideologyProvenance"]
if p is None or p.get("source") != "martin_quinn":
    print(f"FAIL: case(id) round-trip lost provenance: {p!r}", file=sys.stderr)
    sys.exit(1)
print(f"  round-trip OK: source={p['source']} release={p.get('release')}")
PY

echo "==> PASS — date-aware MQ resolver + per-case provenance end-to-end"
