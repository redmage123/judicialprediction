#!/usr/bin/env bash
# Sprint 15 / S15.4 — FJC Biographical Directory ingest wrapper.
#
# Pulls the Federal Judicial Center's free judges.csv (~3500 rows) and
# upserts every confirmed Article III judge into the `judges` table.
# Sprint 14 surfaced that most cafc opinions had `judge_severity = NULL`
# because no panel member appeared in our KG; this run closes that gap.
#
# Usage:
#   scripts/fjc-ingest.sh             # full ingest from the live FJC URL
#   scripts/fjc-ingest.sh --dry-run   # parse only, no DB writes
#   scripts/fjc-ingest.sh --input /tmp/judges.csv
#
# Environment:
#   DATABASE_URL — Postgres DSN (defaults to the dev DSN below).
#   PYTHON       — Python interpreter (defaults to `python3`).

set -euo pipefail

# Auto-detect JP root so the script works regardless of cwd.
JP_ROOT="${JP_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$JP_ROOT"

# Default DSN matches dev docker-compose port-mapping.  Override with
# DATABASE_URL=… for staging/prod.
export DATABASE_URL="${DATABASE_URL:-postgresql://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev}"

PYTHON="${PYTHON:-python3}"
SCRIPT="$JP_ROOT/python/ml-inference-svc/scripts/fjc_ingest.py"

if [ ! -f "$SCRIPT" ]; then
    echo "FAIL: $SCRIPT not found" >&2
    exit 1
fi

echo "==> FJC ingest"
echo "    db: ${DATABASE_URL%%@*}@…"
echo "    py: $($PYTHON --version 2>&1)"
echo

exec "$PYTHON" "$SCRIPT" "$@"
