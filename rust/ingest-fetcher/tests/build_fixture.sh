#!/usr/bin/env bash
# Rebuild tests/fixtures/sample.tar.gz from tests/fixtures/raw/*.json.
# Run from anywhere; resolves paths relative to the script.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAW_DIR="$HERE/fixtures/raw"
OUT="$HERE/fixtures/sample.tar.gz"

cd "$RAW_DIR"
tar -czf "$OUT" --owner=0 --group=0 --numeric-owner --sort=name --mtime='2026-01-01 00:00:00 UTC' op_*.json
echo "wrote $OUT (entries: $(tar -tzf "$OUT" | wc -l))"
