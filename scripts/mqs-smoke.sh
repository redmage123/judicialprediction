#!/bin/bash
# Sprint 8 / S8.7 — Martin-Quinn end-to-end smoke.
#
# Mirrors scripts/dime-smoke.sh.  Verifies that mqs-ingest can ingest a
# synthetic fixture, that at least one judge ends up with bio.mqs, and
# that the gateway's extractFeatures query reports martin_quinn as the
# ideology source for that judge.
#
# Pre-conditions: docker-compose dev stack running; operator credentials
# seeded; at least one judges row exists for the matched justice (the
# script seeds Marshall on SCOTUS as a fallback).
set -euo pipefail

JP="${JP:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$JP"

TENANT_ID="${TENANT_ID:-00000000-0000-0000-0000-000000000001}"
DB_URL="${DATABASE_URL:-postgres://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev}"
WEB_BASE="${WEB_BASE:-http://localhost:3030}"
OPERATOR_EMAIL="${OPERATOR_EMAIL:-dev-tenant1@example.test}"
OPERATOR_PASSWORD="${OPERATOR_PASSWORD:-tenant1-pw}"

echo "==> 1) Build mqs-ingest"
cargo build --release --manifest-path rust/Cargo.toml -p mqs-ingest >/dev/null

echo "==> 2) Ensure a SCOTUS court + Marshall judge exist for the matcher"
docker exec -i judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -q <<SQL >/dev/null
INSERT INTO courts (id, tenant_id, name, jurisdiction, source, source_id)
SELECT gen_random_uuid(), '$TENANT_ID',
       'Supreme Court of the United States', 'us-federal',
       'courtlistener', 'scotus'
WHERE NOT EXISTS (
  SELECT 1 FROM courts
   WHERE tenant_id='$TENANT_ID' AND source_id='scotus'
);
WITH sc AS (SELECT id FROM courts WHERE tenant_id='$TENANT_ID' AND source_id='scotus')
INSERT INTO judges (id, tenant_id, full_name, normalized_name, primary_court_id, bio, source)
SELECT gen_random_uuid(), '$TENANT_ID', 'MARSHALL', 'marshall', sc.id, '{}'::jsonb, 'courtlistener-test'
FROM sc
WHERE NOT EXISTS (
  SELECT 1 FROM judges
   WHERE tenant_id='$TENANT_ID' AND normalized_name='marshall'
);
SQL

echo "==> 3) Run mqs-ingest against the fixture"
DATABASE_URL="$DB_URL" RUST_LOG=info \
  ./rust/target/release/mqs-ingest ingest \
  --csv rust/mqs-ingest/fixtures/mqs-mini.csv \
  --tenant-id "$TENANT_ID" \
  --report /tmp/mqs-smoke-unmatched.tsv \
  2>&1 | tail -3

echo "==> 4) Assert at least one judge has bio.mqs"
COUNT=$(docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA \
  -c "SELECT COUNT(*) FROM judges WHERE tenant_id='$TENANT_ID' AND bio ? 'mqs';")
if [ "$COUNT" -lt 1 ]; then
    echo "FAIL: no judges have bio.mqs after ingest"
    exit 1
fi
echo "    $COUNT judge(s) enriched"

echo "==> 5) Pick a MQ-enriched judge for the extractFeatures probe"
PROBE_JUDGE=$(docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA \
  -c "SELECT upper(full_name) FROM judges WHERE tenant_id='$TENANT_ID' AND bio ? 'mqs' LIMIT 1;" \
  | tr -d ' ')
echo "    probing with '$PROBE_JUDGE'"

echo "==> 6) Authenticate via BFF"
COOKIE=$(mktemp)
trap 'rm -f "$COOKIE"' EXIT
curl -sS -c "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"email\":\"$OPERATOR_EMAIL\",\"password\":\"$OPERATOR_PASSWORD\"}" \
  "$WEB_BASE/api/auth/login" >/dev/null

echo "==> 7) extractFeatures should report martin_quinn"
RESP=$(curl -sS -b "$COOKIE" -X POST -H 'Content-Type: application/json' \
  -d "{\"query\":\"query(\$t: String!) { extractFeatures(text: \$t) { judgeName ideologyDistance ideologySource ideologyRelease ideologyCfscore ideologyTerm } }\",\"variables\":{\"t\":\"$PROBE_JUDGE, Judge: The Court reverses the judgment of the court below.\"}}" \
  "$WEB_BASE/api/graphql")
echo "$RESP" | python3 -m json.tool

python3 - "$RESP" <<'PY'
import json, sys
r = json.loads(sys.argv[1])
ef = r.get("data", {}).get("extractFeatures") or {}
src = ef.get("ideologySource")
rel = ef.get("ideologyRelease")
term = ef.get("ideologyTerm")
cfs = ef.get("ideologyCfscore")
if src != "martin_quinn":
    print(f"FAIL: ideologySource={src!r}, want 'martin_quinn'")
    sys.exit(1)
if rel != "mqs-2023-v1":
    print(f"FAIL: ideologyRelease={rel!r}, want 'mqs-2023-v1'")
    sys.exit(1)
if term is None:
    print("FAIL: ideologyTerm is null (should be set for MQ rows)")
    sys.exit(1)
if cfs is None:
    print("FAIL: ideologyCfscore is null")
    sys.exit(1)
print(f"  source={src}, release={rel}, term={term}, score={cfs}")
PY
echo "==> PASS — MQ flows through to extractFeatures end-to-end"
