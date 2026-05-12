#!/bin/bash
# seed-demo.sh — apply scripts/seed-demo-mix.sql to the local dev postgres.
#
# Inserts 12 hand-crafted cases against the dev tenant so the /cases dashboard
# shows a real Settle / Try / Borderline mix.  See the SQL file header for the
# decision-arith rules each row satisfies.
#
# Defaults assume the docker-compose dev stack (container
# judicialpredict_postgres, DB judicialpredict_dev, user judicialpredict).
# Override via env if you have a different topology.
#
# Usage:
#   pnpm jp:seed-demo
#   # or directly:
#   scripts/seed-demo.sh
#
# Idempotency: each invocation appends 12 rows.  Pass --reset to TRUNCATE the
# cases table first.

set -euo pipefail

CONTAINER="${JP_POSTGRES_CONTAINER:-judicialpredict_postgres}"
DB="${JP_POSTGRES_DB:-judicialpredict_dev}"
USER="${JP_POSTGRES_USER:-judicialpredict}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SQL_FILE="$SCRIPT_DIR/seed-demo-mix.sql"

if [ ! -f "$SQL_FILE" ]; then
  echo "error: $SQL_FILE not found" >&2
  exit 1
fi

if ! docker ps --format '{{.Names}}' | grep -qx "$CONTAINER"; then
  echo "error: container '$CONTAINER' is not running" >&2
  echo "hint: bring the dev stack up first (docker compose -f docker-compose.dev.yml up -d)" >&2
  exit 1
fi

if [ "${1:-}" = "--reset" ]; then
  echo "→ truncating cases table (--reset)"
  docker exec -i "$CONTAINER" psql -U "$USER" -d "$DB" \
    -c "TRUNCATE cases CASCADE;"
fi

echo "→ seeding 12 mixed cases (Settle / Try / Borderline) into $DB on $CONTAINER"
docker exec -i "$CONTAINER" psql -U "$USER" -d "$DB" < "$SQL_FILE"

echo "→ current recommendation mix:"
docker exec -i "$CONTAINER" psql -U "$USER" -d "$DB" -c "
  SELECT recommendation->>'kind' AS kind, COUNT(*)
  FROM   cases
  WHERE  tenant_id = '00000000-0000-0000-0000-000000000001'
  GROUP BY 1
  ORDER BY 1;
"
