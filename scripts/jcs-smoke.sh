#!/bin/bash
# Sprint 9 / S9.7 — JCS end-to-end smoke. Mirrors dime-smoke.sh + mqs-smoke.sh.
set -euo pipefail

JP="${JP:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$JP"

TENANT_ID="${TENANT_ID:-00000000-0000-0000-0000-000000000001}"
DB_URL="${DATABASE_URL:-postgres://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev}"
WEB_BASE="${WEB_BASE:-http://localhost:3030}"
OPERATOR_EMAIL="${OPERATOR_EMAIL:-dev-tenant1@example.test}"
OPERATOR_PASSWORD="${OPERATOR_PASSWORD:-tenant1-pw}"

echo "==> 1) Build jcs-ingest"
cargo build --release --manifest-path rust/Cargo.toml -p jcs-ingest >/dev/null

echo "==> 2) Ensure a ca7 court + Posner judge exist (Circuit court is where JCS earns its keep)"
docker exec -i judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -q <<SQL >/dev/null
INSERT INTO courts (id, tenant_id, name, jurisdiction, source, source_id)
SELECT gen_random_uuid(), '$TENANT_ID',
       'US Court of Appeals, Seventh Circuit', 'us-federal',
       'courtlistener', 'ca7'
WHERE NOT EXISTS (
  SELECT 1 FROM courts WHERE tenant_id='$TENANT_ID' AND source_id='ca7'
);
WITH c AS (SELECT id FROM courts WHERE tenant_id='$TENANT_ID' AND source_id='ca7')
INSERT INTO judges (id, tenant_id, full_name, normalized_name, primary_court_id, bio, source)
SELECT gen_random_uuid(), '$TENANT_ID', 'POSNER', 'posner', c.id, '{}'::jsonb, 'courtlistener-test'
FROM c
WHERE NOT EXISTS (
  SELECT 1 FROM judges WHERE tenant_id='$TENANT_ID' AND normalized_name='posner'
);
SQL

echo "==> 3) Run jcs-ingest against the fixture"
DATABASE_URL="$DB_URL" RUST_LOG=info \
  ./rust/target/release/jcs-ingest ingest \
  --csv rust/jcs-ingest/fixtures/jcs-mini.csv \
  --tenant-id "$TENANT_ID" \
  --report /tmp/jcs-smoke-unmatched.tsv \
  2>&1 | tail -3

echo "==> 4) Assert at least one judge has bio.jcs"
COUNT=$(docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA \
  -c "SELECT COUNT(*) FROM judges WHERE tenant_id='$TENANT_ID' AND bio ? 'jcs';")
if [ "$COUNT" -lt 1 ]; then
    echo "FAIL: no judges have bio.jcs after ingest"
    exit 1
fi
echo "    $COUNT judge(s) enriched"

echo "==> 5) Pick a JCS-only judge (no MQ + no DIME) for the precedence probe"
PROBE_JUDGE=$(docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA \
  -c "SELECT upper(full_name)
        FROM judges
       WHERE tenant_id='$TENANT_ID'
         AND bio ? 'jcs'
         AND NOT (bio ? 'mqs')
         AND NOT (bio ? 'dime')
       LIMIT 1;" | tr -d ' ')
if [ -z "$PROBE_JUDGE" ]; then
    echo "FAIL: no judge with JCS-only enrichment to probe; check fixture / seed"
    exit 1
fi
echo "    probing with '$PROBE_JUDGE'"

echo "==> 6) Authenticate via BFF"
COOKIE=$(mktemp); trap 'rm -f "$COOKIE"' EXIT
curl -sS -c "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"email\":\"$OPERATOR_EMAIL\",\"password\":\"$OPERATOR_PASSWORD\"}" \
  "$WEB_BASE/api/auth/login" >/dev/null

echo "==> 7) extractFeatures should report judicial_common_space"
RESP=$(curl -sS -b "$COOKIE" -X POST -H 'Content-Type: application/json' \
  -d "{\"query\":\"query(\$t: String!) { extractFeatures(text: \$t) { judgeName ideologyDistance ideologySource ideologyRelease ideologyCfscore } }\",\"variables\":{\"t\":\"$PROBE_JUDGE, Judge: The Court reverses the judgment of the court below.\"}}" \
  "$WEB_BASE/api/graphql")
echo "$RESP" | python3 -m json.tool

python3 - "$RESP" <<'PY'
import json, sys
r = json.loads(sys.argv[1])
ef = r.get("data", {}).get("extractFeatures") or {}
src = ef.get("ideologySource")
rel = ef.get("ideologyRelease")
score = ef.get("ideologyCfscore")
if src != "judicial_common_space":
    print(f"FAIL: ideologySource={src!r}, want 'judicial_common_space'")
    sys.exit(1)
if rel != "jcs-2018-v1":
    print(f"FAIL: ideologyRelease={rel!r}, want 'jcs-2018-v1'")
    sys.exit(1)
if score is None:
    print("FAIL: ideologyCfscore is null"); sys.exit(1)
print(f"  source={src}, release={rel}, score={score}")
PY
echo "==> PASS — JCS flows through to extractFeatures end-to-end"
