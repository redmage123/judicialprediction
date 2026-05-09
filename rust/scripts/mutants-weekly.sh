#!/usr/bin/env bash
# JudicialPredict — Weekly cargo-mutants survey
# Runs on each functional-core crate, diffs against the baseline, and
# posts a summary to Slack (if SLACK_WEBHOOK_URL is set) or appends to
# /var/log/jp-mutants-weekly.log.
#
# Install: see rust/README.md "Mutation testing" section.
# Cron:    see rust/scripts/mutants-weekly.cron

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BASELINE_JSON="$WORKSPACE_DIR/.mutants-baseline.json"
LOG_FILE="/var/log/jp-mutants-weekly.log"
TIMEOUT_SECS=1800
CRATES=(decision-arith monte-carlo-sim rate-limit feature-deriver)

# ── helpers ──────────────────────────────────────────────────────────────────

log() { echo "[$(date -u '+%Y-%m-%dT%H:%M:%SZ')] $*"; }

extract_counts() {
    local out_dir="$1"
    local caught=0 missed=0 unviable=0

    # cargo-mutants writes a mutations.json in the output directory
    local json="$out_dir/mutations.json"
    if [[ -f "$json" ]]; then
        caught=$(python3 -c "import json,sys; d=json.load(open('$json')); print(sum(1 for m in d if m.get('outcome')=='caught'))" 2>/dev/null || echo 0)
        missed=$(python3 -c "import json,sys; d=json.load(open('$json')); print(sum(1 for m in d if m.get('outcome')=='missed'))" 2>/dev/null || echo 0)
        unviable=$(python3 -c "import json,sys; d=json.load(open('$json')); print(sum(1 for m in d if m.get('outcome')=='unviable'))" 2>/dev/null || echo 0)
    fi
    echo "$caught $missed $unviable"
}

# ── main ─────────────────────────────────────────────────────────────────────

cd "$WORKSPACE_DIR"

# Source cargo env if running via cron (PATH may not include ~/.cargo/bin)
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"

TIMESTAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
SUMMARY_LINES=()
SUMMARY_LINES+=("## JudicialPredict — Weekly Mutation Test Report ($TIMESTAMP)")
SUMMARY_LINES+=("")

ALL_OK=true
declare -A NEW_COUNTS

for crate in "${CRATES[@]}"; do
    log "Running cargo-mutants on $crate (timeout ${TIMEOUT_SECS}s)..."
    OUT_DIR=".mutants-$crate"
    mkdir -p "$OUT_DIR"

    set +e
    cargo mutants -p "$crate" --no-shuffle --timeout "$TIMEOUT_SECS" --output "$OUT_DIR/" 2>&1
    EXIT_CODE=$?
    set -e

    if [[ $EXIT_CODE -ne 0 ]]; then
        log "WARN: cargo mutants exited $EXIT_CODE for $crate (may indicate timeout or partial run)"
        ALL_OK=false
    fi

    read -r caught missed unviable <<< "$(extract_counts "$OUT_DIR")"
    total=$(( caught + missed + unviable ))
    NEW_COUNTS[$crate]="$caught $missed $unviable"

    # Diff against baseline
    BASELINE_MISSED=null
    if [[ -f "$BASELINE_JSON" ]]; then
        BASELINE_MISSED=$(python3 -c "
import json, sys
d = json.load(open('$BASELINE_JSON'))
v = d.get('crates', {}).get('$crate')
print(v['missed'] if v else 'null')
" 2>/dev/null || echo "null")
    fi

    if [[ "$BASELINE_MISSED" == "null" ]]; then
        STATUS="🆕 no baseline — establishing"
    elif [[ $missed -gt $BASELINE_MISSED ]]; then
        STATUS="🔴 REGRESSION: missed $missed (was $BASELINE_MISSED)"
        ALL_OK=false
    elif [[ $missed -lt $BASELINE_MISSED ]]; then
        STATUS="🟢 improved: missed $missed (was $BASELINE_MISSED)"
    else
        STATUS="✅ no regression: missed $missed"
    fi

    SUMMARY_LINES+=("### $crate")
    SUMMARY_LINES+=("- caught: $caught | missed: $missed | unviable: $unviable | total: $total")
    SUMMARY_LINES+=("- $STATUS")
    SUMMARY_LINES+=("")

    log "$crate: caught=$caught missed=$missed unviable=$unviable → $STATUS"
done

# ── update baseline JSON if all crates ran successfully ──────────────────────

if [[ "$ALL_OK" == "true" ]]; then
    python3 - <<PYEOF
import json, datetime

baseline = {}
try:
    baseline = json.load(open("$BASELINE_JSON"))
except Exception:
    baseline = {"crates": {}, "generated_at": ""}

for crate, counts in [
$(for c in "${CRATES[@]}"; do
    read -r caught missed unviable <<< "${NEW_COUNTS[$c]}"
    echo "    (\"$c\", ($caught, $missed, $unviable)),"
done)
]:
    baseline.setdefault("crates", {})[crate] = {
        "caught": counts[0], "missed": counts[1], "unviable": counts[2]
    }

baseline["generated_at"] = "$TIMESTAMP"
json.dump(baseline, open("$BASELINE_JSON", "w"), indent=2)
print("Baseline JSON updated.")
PYEOF
    log "Baseline JSON updated at $BASELINE_JSON"
fi

SUMMARY_TEXT="$(printf '%s\n' "${SUMMARY_LINES[@]}")"

# ── deliver the summary ───────────────────────────────────────────────────────

if [[ -n "${SLACK_WEBHOOK_URL:-}" ]]; then
    PAYLOAD=$(python3 -c "
import json, sys
text = sys.stdin.read()
print(json.dumps({'text': text}))
" <<< "$SUMMARY_TEXT")
    curl -s -X POST -H 'Content-type: application/json' --data "$PAYLOAD" "$SLACK_WEBHOOK_URL"
    log "Summary posted to Slack."
else
    {
        echo ""
        echo "======================================================="
        echo "$SUMMARY_TEXT"
        echo "======================================================="
    } >> "$LOG_FILE" 2>/dev/null || {
        # Log dir not writable — print to stdout instead
        echo "$SUMMARY_TEXT"
    }
    log "No SLACK_WEBHOOK_URL set — summary written to $LOG_FILE (or stdout)."
fi

log "Weekly mutants survey complete."
