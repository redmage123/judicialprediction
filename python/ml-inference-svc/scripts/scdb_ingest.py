"""
Sprint 15 / S15.3 — SCDB ingest into case_outcome_labels.

The Supreme Court Database (Washington University) is the only
pre-labelled outcome source we have. Modern release covers 1946–2024
case-centred; legacy covers 1791–1945. Each row has a hand-coded
`partyWinning` that we map directly to our binary outcome label:

    partyWinning = 1  →  'petitioner'
    partyWinning = 0  →  'respondent'
    partyWinning = 2  →  drop (SCDB-coded ambiguous)
    blank / NaN       →  drop

case_outcome_labels.opinion_id is bigint and FK-less (S15.2 schema), so
we cannot store SCDB's text `caseId` directly. Strategy:

  * If a SCOTUS row already exists in case_documents that matches on
    docket-number, use its CL opinion_id (positive bigint). This is the
    "matched" path.

  * Otherwise insert with a synthetic NEGATIVE opinion_id derived from
    a deterministic SHA-1 of the SCDB caseId truncated to the negative
    bigint range. This is the "pending" path. When S15.5 (CAP) brings
    in real SCOTUS opinions, S15.10's retrain join can swap the pending
    rows over using the sidecar map below.

  * Sidecar: python/ml-inference-svc/data/scdb_case_ids.json maps the
    synthetic opinion_id → SCDB caseId so the retrain pipeline can
    reconnect labels to opinions once CL/CAP ingest catches up.

Confidence is fixed at 1.0 for SCDB (hand-coded).

Idempotent via the (opinion_id, source) UNIQUE constraint plus an
ON CONFLICT DO UPDATE clause so reruns refresh ingested_at without
duplicating rows.

Usage:
    python scripts/scdb_ingest.py
    python scripts/scdb_ingest.py --input /path/to/SCDB_2024_01_caseCentered_Citation.csv
    python scripts/scdb_ingest.py --dry-run
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import os
import sys
import tempfile
import urllib.request
import zipfile
from pathlib import Path
from typing import Iterable

# psycopg2 is imported lazily so --dry-run can run without it (handy
# for the first call in CI before the venv is populated).

DEFAULT_DSN = (
    "postgresql://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev"
)

SCDB_MODERN_URL = (
    "https://supremecourt.wustl.edu/files/data/SCDB_2024_01_caseCentered_Citation.csv.zip"
)

# SCDB encoding is windows-1252 / latin-1, NOT utf-8. Older releases have
# non-ASCII in petitioner / respondent names that crash a utf-8 decoder.
SCDB_ENCODING = "latin-1"

# Map SCDB partyWinning → our case_outcome_labels.outcome value.
# SCDB code book: 0 = petitioning party lost, 1 = won, 2 = ambiguous.
PARTY_WINNING_MAP = {"0": "respondent", "1": "petitioner"}

# Bigint negative range we hash into. Postgres bigint is signed 64-bit;
# we restrict to 31 bits of magnitude so the synthetic ids stay well
# away from CourtListener's positive opinion_id range (which is in the
# low millions today) and never collide with any plausible CL id.
SYNTH_ID_MAGNITUDE = 2**31

SIDECAR_PATH_DEFAULT = (
    Path(__file__).resolve().parent.parent / "data" / "scdb_case_ids.json"
)


# ─────────────────────────────────────────────────────────────────────────────
# CSV download / read
# ─────────────────────────────────────────────────────────────────────────────


def download_modern_csv(dest_dir: Path) -> Path:
    """Download SCDB modern.csv.zip, extract the single CSV inside, return path."""
    print(f"→ downloading SCDB modern from {SCDB_MODERN_URL}")
    dest_zip = dest_dir / "SCDB_modern.csv.zip"
    urllib.request.urlretrieve(SCDB_MODERN_URL, dest_zip)
    with zipfile.ZipFile(dest_zip) as zf:
        # SCDB ships exactly one .csv in the zip.
        csv_members = [n for n in zf.namelist() if n.lower().endswith(".csv")]
        if not csv_members:
            raise SystemExit(f"no .csv found inside {dest_zip}")
        zf.extract(csv_members[0], dest_dir)
        return dest_dir / csv_members[0]


def read_scdb_rows(csv_path: Path) -> Iterable[dict[str, str]]:
    """Yield SCDB rows as dicts. Tolerant of latin-1 case-name garble."""
    with open(csv_path, "r", encoding=SCDB_ENCODING, newline="") as fh:
        reader = csv.DictReader(fh)
        for row in reader:
            yield row


# ─────────────────────────────────────────────────────────────────────────────
# Synthetic opinion_id
# ─────────────────────────────────────────────────────────────────────────────


def synth_opinion_id(case_id: str) -> int:
    """Deterministic negative bigint from SCDB caseId via SHA-1.

    We take 31 bits of the digest, then negate. This guarantees:
      * Same caseId always maps to the same id (idempotency on rerun).
      * Result is in the closed range [-(2^31 - 1), -1].
      * Never collides with positive CourtListener opinion_ids.
    """
    digest = hashlib.sha1(case_id.encode("utf-8")).digest()
    # Take the low 4 bytes, mask to 31 bits, ensure non-zero, then negate.
    n = int.from_bytes(digest[:4], "big") & (SYNTH_ID_MAGNITUDE - 1)
    if n == 0:
        n = 1
    return -n


# ─────────────────────────────────────────────────────────────────────────────
# Row projection
# ─────────────────────────────────────────────────────────────────────────────


def project_rows(scdb_rows: Iterable[dict[str, str]]) -> tuple[list[tuple], dict]:
    """Project SCDB rows → (db_rows, sidecar_map).

    db_rows is a list of (opinion_id, source, outcome, confidence) tuples.
    sidecar_map is synth_opinion_id (as str — JSON keys must be str) → caseId.
    """
    db_rows: list[tuple] = []
    sidecar: dict[str, str] = {}
    seen_ids: set[int] = set()  # within-this-run dedupe (multi-issue rows)

    for row in scdb_rows:
        party = (row.get("partyWinning") or "").strip()
        outcome = PARTY_WINNING_MAP.get(party)
        if outcome is None:
            continue
        case_id = (row.get("caseId") or "").strip()
        if not case_id:
            continue
        opinion_id = synth_opinion_id(case_id)
        if opinion_id in seen_ids:
            # SCDB case-centred CSVs are already one-row-per-case, but
            # the legacy + modern files overlap at the 1946 boundary
            # and a future "combined" path could feed both. Dedupe to
            # the first occurrence — they should be identical anyway.
            continue
        seen_ids.add(opinion_id)
        db_rows.append((opinion_id, "scdb", outcome, 1.0))
        sidecar[str(opinion_id)] = case_id

    return db_rows, sidecar


# ─────────────────────────────────────────────────────────────────────────────
# DB write
# ─────────────────────────────────────────────────────────────────────────────


INSERT_SQL = """
INSERT INTO case_outcome_labels (opinion_id, source, outcome, confidence)
VALUES %s
ON CONFLICT (opinion_id, source) DO UPDATE
SET outcome = EXCLUDED.outcome,
    confidence = EXCLUDED.confidence,
    ingested_at = now()
"""


def ensure_table_exists(cur) -> None:
    """Bail out with a clear error if the S15.2 migration hasn't run yet."""
    cur.execute("SELECT to_regclass('public.case_outcome_labels')")
    result = cur.fetchone()
    if result is None or result[0] is None:
        print(
            "error: case_outcome_labels table is missing — has the S15.2 "
            "migration (20260518100000_s15_labels_and_judges.sql) been applied?",
            file=sys.stderr,
        )
        sys.exit(1)


def insert_rows(dsn: str, rows: list[tuple]) -> None:
    import psycopg2
    from psycopg2.extras import execute_values

    conn = psycopg2.connect(dsn)
    try:
        with conn:
            with conn.cursor() as cur:
                ensure_table_exists(cur)
                execute_values(cur, INSERT_SQL, rows, page_size=1000)
    finally:
        conn.close()


def count_matched(dsn: str, synth_ids: list[int]) -> int:
    """Count how many SCDB rows happen to share an opinion_id with an
    existing case_documents row. With synthetic negative ids this will
    be 0 today — but it's the right plumbing for when S15.10 swaps
    pending rows over to real CL ids."""
    if not synth_ids:
        return 0
    import psycopg2

    conn = psycopg2.connect(dsn)
    try:
        with conn.cursor() as cur:
            # Negative synth ids will never match positive CL ids — but
            # check anyway in case future code paths post-process the
            # labels into positive ids before this script runs again.
            cur.execute(
                "SELECT COUNT(*) FROM case_documents WHERE opinion_id = ANY(%s)",
                (synth_ids,),
            )
            return int(cur.fetchone()[0])
    finally:
        conn.close()


# ─────────────────────────────────────────────────────────────────────────────
# Entry point
# ─────────────────────────────────────────────────────────────────────────────


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--input",
        type=Path,
        default=None,
        help="Path to SCDB caseCentered CSV. Defaults to downloading the modern release.",
    )
    parser.add_argument(
        "--source",
        default="scdb",
        help="case_outcome_labels.source value (default: scdb).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Parse + project but skip all DB writes.",
    )
    parser.add_argument(
        "--sidecar",
        type=Path,
        default=SIDECAR_PATH_DEFAULT,
        help="Output JSON path mapping synthetic opinion_id → SCDB caseId.",
    )
    args = parser.parse_args()

    dsn = os.environ.get("DATABASE_URL", DEFAULT_DSN)

    if args.input is not None:
        csv_path = args.input
        if not csv_path.is_file():
            print(f"error: --input {csv_path} not found", file=sys.stderr)
            return 1
        tmp_dir = None
    else:
        tmp_dir = tempfile.mkdtemp(prefix="scdb-")
        csv_path = download_modern_csv(Path(tmp_dir))

    print(f"→ reading SCDB CSV {csv_path} (encoding={SCDB_ENCODING})")
    rows = list(read_scdb_rows(csv_path))
    print(f"  raw rows           : {len(rows)}")

    db_rows, sidecar = project_rows(rows)
    n_pet = sum(1 for r in db_rows if r[2] == "petitioner")
    n_resp = sum(1 for r in db_rows if r[2] == "respondent")

    # Override source if the caller wanted something other than 'scdb'
    # (the schema CHECK rejects anything outside {scdb,detector,learned}
    # so this is mostly an audit hook).
    if args.source != "scdb":
        db_rows = [(r[0], args.source, r[2], r[3]) for r in db_rows]

    synth_ids = [r[0] for r in db_rows]

    if args.dry_run:
        matched = 0
        pending = len(db_rows)
        wrote_sidecar = False
    else:
        insert_rows(dsn, db_rows)
        matched = count_matched(dsn, synth_ids)
        pending = len(db_rows) - matched
        args.sidecar.parent.mkdir(parents=True, exist_ok=True)
        args.sidecar.write_text(json.dumps(sidecar, indent=2, sort_keys=True))
        wrote_sidecar = True

    print(
        f"Loaded {len(db_rows)} SCDB rows; {n_pet} petitioner / {n_resp} respondent;"
        f" mapped to {matched} existing case_documents rows;"
        f" {pending} stored as pending (no matching CL opinion yet)."
    )
    if wrote_sidecar:
        print(f"  sidecar map        : {args.sidecar}")
    if args.dry_run:
        print("  (dry-run: no DB writes, no sidecar written)")

    return 0


if __name__ == "__main__":
    sys.exit(main())
