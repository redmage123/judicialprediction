#!/bin/bash
# JudicialPredict — recurring sync of Plane / RAG / KG / CMS
# Runs every 30 min via cron.

set -uo pipefail

PROJECT_DIR="/opt/ai-elevate/gigforge/projects/judicialpredict"
RAG_KEY="rag_ak_aielevate_2026_secret"
RAG_BASE="http://localhost:8020/api/v1"
# Source rotating Plane creds (auto-refreshed daily by /opt/ai-elevate/cron/plane-token-refresh.sh)
# shellcheck source=/dev/null
[ -r /opt/ai-elevate/credentials/plane.env ] && . /opt/ai-elevate/credentials/plane.env
PLANE_TOKEN="${PLANE_GF_TOKEN:?PLANE_GF_TOKEN not set; check /opt/ai-elevate/credentials/plane.env}"
PLANE_BASE="${PLANE_GF_URL:-http://localhost:8801}/api/v1/workspaces/gigforge/projects/92ad0116-cbac-4975-ac87-4ea820c0be96"
LOG="/var/log/jp-sync.log"
MARKER_DIR="/var/lib/jp-sync"
mkdir -p "$MARKER_DIR"

log() { echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" >> "$LOG"; }

# 1. RAG re-ingest of changed spec / ADRs / handoffs / sprint-boards / reports
sync_rag() {
    local file="$1"
    local hash_file="$MARKER_DIR/$(echo "$file" | tr / _).sha256"
    if [[ ! -f "$file" ]]; then return; fi
    local current=$(sha256sum "$file" | awk '{print $1}')
    local previous=$(cat "$hash_file" 2>/dev/null || echo "")
    if [[ "$current" == "$previous" ]]; then return; fi
    log "RAG sync: $file"
    curl -sf -X POST "$RAG_BASE/ingest/file" \
        -H "Authorization: Bearer $RAG_KEY" \
        -F "file=@$file" \
        -F "org_slug=gigforge" \
        -F "collection_slug=engineering" \
        -F "metadata={\"project\":\"JudicialPredict\",\"path\":\"$file\",\"synced_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" \
        > /dev/null \
        && echo "$current" > "$hash_file" \
        && log "  ingested OK" \
        || log "  ingest FAILED"
}

find "$PROJECT_DIR" -type f \( -name '*.md' -o -name '*.adr' \) 2>/dev/null | while read f; do
    sync_rag "$f"
done

# 2. Plane → markdown export of issue/state changes (read-only mirror for KG)
EXPORT_DIR="$PROJECT_DIR/.plane-mirror"
mkdir -p "$EXPORT_DIR"
curl -sf "$PLANE_BASE/issues/?per_page=200" \
    -H "X-Api-Key: $PLANE_TOKEN" \
    > "$EXPORT_DIR/issues.json.tmp" \
    && mv "$EXPORT_DIR/issues.json.tmp" "$EXPORT_DIR/issues.json" \
    && log "Plane issues mirrored (issues.json)"

curl -sf "$PLANE_BASE/cycles/?per_page=50" \
    -H "X-Api-Key: $PLANE_TOKEN" \
    > "$EXPORT_DIR/cycles.json.tmp" \
    && mv "$EXPORT_DIR/cycles.json.tmp" "$EXPORT_DIR/cycles.json" \
    && log "Plane cycles mirrored (cycles.json)"

# 3. KG entity update (project status snapshot)
SNAPSHOT="$EXPORT_DIR/kg-snapshot.json"
python3 - <<PYEOF > "$SNAPSHOT"
import json, datetime
issues_file = "$EXPORT_DIR/issues.json"
try:
    with open(issues_file) as f:
        d = json.load(f)
    issues = d.get("results", []) if isinstance(d, dict) else d
    state_counts = {}
    agent_counts = {}
    for i in issues:
        # state name lookup omitted — Plane returns state_id; counts by state_id
        state_counts[str(i.get("state"))] = state_counts.get(str(i.get("state")), 0) + 1
    snapshot = {
        "project": "JudicialPredict",
        "plane_id": "92ad0116-cbac-4975-ac87-4ea820c0be96",
        "synced_at": datetime.datetime.utcnow().isoformat() + "Z",
        "total_issues": len(issues),
        "state_counts": state_counts,
    }
    print(json.dumps(snapshot, indent=2))
except Exception as e:
    print(json.dumps({"error": str(e)}))
PYEOF
log "KG snapshot updated"

# 4. CMS — currently file-based projects.ts already updated; nothing recurring needed.
#    On milestone completion, gigforge-pm will edit projects.ts directly to update badges.

log "Sync cycle complete."
