#!/bin/bash
# Daily CourtListener ingest — runs `ingest-fetcher run-rest` for TWO courts
# per day, bounded by CourtListener's 125/day REST quota.
#
# Schedule: cron at 04:00 UTC daily.  See docs/runbooks/data-ingest.md.
# Each court walks one search page further back into history per run via the
# per-court `filed_before` cursor (load_oldest_date_filed in
# rust/ingest-fetcher/src/rest.rs); within a court, opinion_id de-dup
# (load_existing_opinion_ids) skips anything already stored.
#
# S6.9: two courts per run.  S4.11 rotated a single court per day, which left
# cafc/bia/scotus growing only ~1 search page per WEEK.  We now run `tax` (the
# high-volume workhorse) plus one rotating minor court every day, with a
# per-court TARGET small enough that the pair stays under the 125/day cap:
#   per court  ~= TARGET hydrate calls + ceil(TARGET*2 / 20) search pages
#   TARGET=50  -> ~50 + 5 = ~55 calls/court -> ~110 calls for the pair.
# If the first court hits the daily cap, the rest are skipped (a second run
# then would only 429 immediately).
#
# Exit policy: 0 on success OR daily-cap-hit (the run did its part);
# non-zero only on real errors (network, DB, build) from any court.
set -uo pipefail

JP=/opt/ai-elevate/gigforge/projects/judicialpredict
# LOG override: lets a smoke test write to a scratch file instead of the
# production log.  Defaults to the production log.
LOG="${LOG:-/var/log/jp-courtlistener-daily.log}"

# S6.9: each day runs `tax` plus one rotating minor court so cafc/bia/scotus
# all keep growing.  Selector is day-of-week mod 3; over a 7-day week the
# minor slot lands cafc x3, bia x2, scotus x2 — every minor court runs at
# least twice a week instead of once.
case "$(( $(date -u +%w) % 3 ))" in
    0) MINOR_COURT="cafc" ;;    # US Federal Circuit (patent + IP)
    1) MINOR_COURT="bia" ;;     # Bd. of Immigration Appeals
    2) MINOR_COURT="scotus" ;;  # Supreme Court
esac
# COURTS env override: space-separated list for manual runs, e.g.
# COURTS="tax scotus" ./scripts/courtlistener-daily.sh
COURTS_SPEC="${COURTS:-tax $MINOR_COURT}"
read -r -a COURT_LIST <<< "$COURTS_SPEC"
TARGET="${TARGET:-50}"  # see header: 2 courts x ~55 calls = ~110, under the 125/day cap
# FETCHER override: lets a smoke test point at a stub instead of burning
# live CourtListener quota.  Defaults to the release binary.
FETCHER="${FETCHER:-./rust/target/release/ingest-fetcher}"

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

cd "$JP"

count_rows() {
    docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA \
        -c "SELECT COUNT(*) FROM case_documents WHERE court_id='$1';" 2>/dev/null || echo "?"
}

COURTS_CSV="$(IFS=,; echo "${COURT_LIST[*]}")"
echo "$(date -u +%FT%TZ) RUN-START courts=$COURTS_CSV target=$TARGET" >> "$LOG"

OVERALL_RC=0
TOTAL_ADDED=0
CAP_HIT=0

for COURT in "${COURT_LIST[@]}"; do
    # Skip the rest of the rotation once the shared daily cap has fired —
    # a second court would only burn a cron slot 429-ing on its first call.
    if [ "$CAP_HIT" -eq 1 ]; then
        echo "$(date -u +%FT%TZ) SKIP  court=$COURT reason=daily_cap_hit" >> "$LOG"
        continue
    fi

    START_COUNT=$(count_rows "$COURT")
    echo "$(date -u +%FT%TZ) START court=$COURT target=$TARGET start_count=$START_COUNT" >> "$LOG"

    # Tee live output to the log AND a temp file so we can inspect the
    # one-line summary for daily_cap_hit without racing a background tee.
    OUTPUT_TMP=$(mktemp)
    "$FETCHER" run-rest "$COURT" --target "$TARGET" 2>&1 \
        | tee -a "$LOG" > "$OUTPUT_TMP"
    RC=${PIPESTATUS[0]}

    if grep -q "daily_cap_hit=true" "$OUTPUT_TMP"; then
        CAP_HIT=1
    fi
    rm -f "$OUTPUT_TMP"

    END_COUNT=$(count_rows "$COURT")
    if [[ "$START_COUNT" =~ ^[0-9]+$ && "$END_COUNT" =~ ^[0-9]+$ ]]; then
        ADDED=$((END_COUNT - START_COUNT))
        TOTAL_ADDED=$((TOTAL_ADDED + ADDED))
    else
        ADDED="?"
    fi
    echo "$(date -u +%FT%TZ) END   court=$COURT rc=$RC added=$ADDED total=$END_COUNT cap_hit=$CAP_HIT" >> "$LOG"

    # Real errors (non-zero exit) propagate; a clean daily-cap exit is rc=0.
    if [ "$RC" -ne 0 ]; then
        OVERALL_RC="$RC"
    fi
done

echo "$(date -u +%FT%TZ) RUN-END   courts=$COURTS_CSV total_added=$TOTAL_ADDED rc=$OVERALL_RC cap_hit=$CAP_HIT" >> "$LOG"
exit "$OVERALL_RC"
