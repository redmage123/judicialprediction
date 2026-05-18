#!/usr/bin/env bash
# scdb-ingest.sh — Sprint 15 / S15.3
#
# Thin wrapper around python/ml-inference-svc/scripts/scdb_ingest.py.
# Downloads SCDB modern.csv (1946–2024) by default, projects each row
# onto case_outcome_labels(source = 'scdb'). All flags forwarded.
#
# Idempotent: re-runs upsert via the (opinion_id, source) UNIQUE
# constraint.
#
# Usage:
#   scripts/scdb-ingest.sh              # download + ingest
#   scripts/scdb-ingest.sh --dry-run    # parse only, no DB writes
#   scripts/scdb-ingest.sh --input /path/to/SCDB_modern.csv

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

PY_DIR="$REPO_ROOT/python/ml-inference-svc"
PY_BIN="$PY_DIR/.venv/bin/python"
PY_SCRIPT="$PY_DIR/scripts/scdb_ingest.py"

if [ ! -x "$PY_BIN" ]; then
  echo "error: ml-inference-svc venv python not found at $PY_BIN" >&2
  echo "hint:  cd python/ml-inference-svc && uv sync (or python -m venv .venv && pip install -e .)" >&2
  exit 1
fi

if [ ! -f "$PY_SCRIPT" ]; then
  echo "error: $PY_SCRIPT not found" >&2
  exit 1
fi

# Dev defaults — DATABASE_URL wins if the operator already exported one.
export DATABASE_URL="${DATABASE_URL:-postgresql://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev}"

exec "$PY_BIN" "$PY_SCRIPT" "$@"
