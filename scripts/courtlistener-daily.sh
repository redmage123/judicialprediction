#!/bin/bash
# Daily CourtListener ingest — runs ingest-fetcher run-rest for tax court,
# bounded by CourtListener's 125/day quota.
#
# Schedule: cron at 04:00 UTC daily.  See docs/runbooks/data-ingest.md.
# Each run picks up where the previous left off via opinion_id de-dup
# (load_existing_opinion_ids in rust/ingest-fetcher/src/rest.rs).
#
# Exit policy: 0 on success OR daily-cap-hit (the run did its part);
# non-zero only on real errors (network, DB, build).
set -uo pipefail

JP=/opt/ai-elevate/gigforge/projects/judicialpredict
LOG=/var/log/jp-courtlistener-daily.log
COURT="${COURT:-tax}"
TARGET="${TARGET:-100}"  # 100 hydrate + ~6 search pages = ~106 API calls, safe under the global 125/day cap

# Source the API token from the credentials file (auto-rotation lives here).
if [ ! -r /opt/ai-elevate/credentials/courtlistener.env ]; then
    echo "$(date -u +%FT%TZ) FATAL no /opt/ai-elevate/credentials/courtlistener.env" >> "$LOG"
    exit 2
fi
set -a
. /opt/ai-elevate/credentials/courtlistener.env
set +a

export DATABASE_URL="postgres://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev"
export RUST_LOG="${RUST_LOG:-info}"

START_COUNT=$(docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA \
    -c "SELECT COUNT(*) FROM case_documents WHERE court_id='$COURT';" 2>/dev/null || echo "?")

echo "$(date -u +%FT%TZ) START court=$COURT target=$TARGET start_count=$START_COUNT" >> "$LOG"

cd "$JP"
./rust/target/release/ingest-fetcher run-rest "$COURT" --target "$TARGET" >> "$LOG" 2>&1
RC=$?

END_COUNT=$(docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA \
    -c "SELECT COUNT(*) FROM case_documents WHERE court_id='$COURT';" 2>/dev/null || echo "?")

ADDED=$((END_COUNT - START_COUNT))
echo "$(date -u +%FT%TZ) END   court=$COURT rc=$RC added=$ADDED total=$END_COUNT" >> "$LOG"
exit "$RC"
