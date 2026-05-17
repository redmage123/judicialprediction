#!/bin/bash
# Sprint 7 / S7.7 — DIME end-to-end smoke.
#
# Verifies:
#   1. dime-ingest builds and ingests the synthetic fixture.
#   2. At least one judge is enriched with a bio.dime cfscore.
#   3. The gateway's extractFeatures query returns the DIME provenance
#      fields when called against an opinion text whose judge has DIME data.
#
# Pre-conditions:
#   * Postgres reachable on the published dev port (compose port-mapped to
#     localhost:5454 by default).
#   * api-gateway up.
#   * Operator credentials seeded (`seed_dev_operators`).
#
# Run from the JP repo root: `bash scripts/dime-smoke.sh`
set -euo pipefail

# Auto-detect JP root so the script works regardless of cwd.
JP="${JP:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$JP"

TENANT_ID="${TENANT_ID:-00000000-0000-0000-0000-000000000001}"
DB_URL="${DATABASE_URL:-postgres://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev}"
WEB_BASE="${WEB_BASE:-http://localhost:3030}"
OPERATOR_EMAIL="${OPERATOR_EMAIL:-dev-tenant1@example.test}"
OPERATOR_PASSWORD="${OPERATOR_PASSWORD:-tenant1-pw}"

echo "==> 1) Build dime-ingest"
cargo build --release --manifest-path rust/Cargo.toml -p dime-ingest >/dev/null

echo "==> 1.5) Seed a DIME-only judge (Foley on tax court). Used in step 4 below"
echo "         to probe the DIME-only branch — MQ + JCS fixtures don't include Foley."
docker exec -i judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -q <<SQL >/dev/null
INSERT INTO courts (id, tenant_id, name, jurisdiction, source, source_id)
SELECT gen_random_uuid(), '$TENANT_ID',
       'United States Tax Court', 'us-federal', 'courtlistener', 'tax'
WHERE NOT EXISTS (
  SELECT 1 FROM courts WHERE tenant_id='$TENANT_ID' AND source_id='tax'
);
WITH c AS (SELECT id FROM courts WHERE tenant_id='$TENANT_ID' AND source_id='tax')
INSERT INTO judges (id, tenant_id, full_name, normalized_name, primary_court_id, bio, source)
SELECT gen_random_uuid(), '$TENANT_ID', 'FOLEY', 'foley', c.id, '{}'::jsonb, 'courtlistener-test'
FROM c
WHERE NOT EXISTS (
  SELECT 1 FROM judges WHERE tenant_id='$TENANT_ID' AND normalized_name='foley'
);
SQL

echo "==> 2) Run ingest against the synthetic fixture"
DATABASE_URL="$DB_URL" RUST_LOG=info \
  ./rust/target/release/dime-ingest ingest \
  --csv rust/dime-ingest/fixtures/dime-judges-mini.csv \
  --tenant-id "$TENANT_ID" \
  --report /tmp/dime-smoke-unmatched.tsv \
  2>&1 | tail -3

echo "==> 3) Assert at least one judge has bio.dime"
COUNT=$(docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA \
  -c "SELECT COUNT(*) FROM judges WHERE tenant_id='$TENANT_ID' AND bio ? 'dime';")
if [ "$COUNT" -lt 1 ]; then
    echo "FAIL: no judges have bio.dime after ingest"
    exit 1
fi
echo "    $COUNT judge(s) enriched"

echo "==> 4) Pick a DIME-ONLY judge for the extractFeatures probe (the gateway's"
echo "       MQ > JCS > DIME precedence means a judge with multiple sources"
echo "       wouldn't surface DIME; we need to exercise the DIME-only branch)."
PROBE_JUDGE=$(docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA \
  -c "SELECT upper(full_name)
        FROM judges
       WHERE tenant_id='$TENANT_ID'
         AND bio ? 'dime'
         AND NOT (bio ? 'mqs')
         AND NOT (bio ? 'jcs')
       LIMIT 1;" | tr -d ' ')
if [ -z "$PROBE_JUDGE" ]; then
    echo "FAIL: no judge has DIME-only enrichment (all DIME judges also have"
    echo "      MQ or JCS data, so the DIME branch isn't exercised). Either"
    echo "      ingest a DIME-only judge first, or rely on the dime parser"
    echo "      tests for parser-level coverage."
    exit 1
fi
echo "    probing with '$PROBE_JUDGE'"

echo "==> 5) Authenticate via BFF"
COOKIE=$(mktemp)
trap 'rm -f "$COOKIE"' EXIT
curl -sS -c "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"email\":\"$OPERATOR_EMAIL\",\"password\":\"$OPERATOR_PASSWORD\"}" \
  "$WEB_BASE/api/auth/login" >/dev/null

echo "==> 6) extractFeatures should return DIME provenance"
RESP=$(curl -sS -b "$COOKIE" -X POST -H 'Content-Type: application/json' \
  -d "{\"query\":\"query(\$t: String!) { extractFeatures(text: \$t) { judgeName ideologyDistance ideologySource ideologyRelease ideologyCfscore } }\",\"variables\":{\"t\":\"$PROBE_JUDGE, Judge: The Service determined a deficiency in petitioner income tax.\"}}" \
  "$WEB_BASE/api/graphql")
echo "$RESP" | python3 -m json.tool

# Parse the JSON properly rather than grepping (jq isn't always installed).
python3 - "$RESP" <<'PY'
import json, sys
r = json.loads(sys.argv[1])
ef = r.get("data", {}).get("extractFeatures") or {}
src = ef.get("ideologySource")
rel = ef.get("ideologyRelease")
cfs = ef.get("ideologyCfscore")
if src != "bonica_dime":
    print(f"FAIL: ideologySource={src!r}, want 'bonica_dime'")
    sys.exit(1)
if rel != "dime-2014-judges-v1.0":
    print(f"FAIL: ideologyRelease={rel!r}, want 'dime-2014-judges-v1.0'")
    sys.exit(1)
if cfs is None:
    print("FAIL: ideologyCfscore is null")
    sys.exit(1)
print(f"  source={src}, release={rel}, cfscore={cfs}")
PY
echo "==> PASS — DIME flows through to extractFeatures end-to-end"
