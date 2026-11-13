"""
S22.4 — NJ/CA state-court ingest, staged.

The CourtListener bulk system no longer publishes per-court tarballs (every
legacy `https://www.courtlistener.com/api/bulk-data/opinions/{court}.tar.gz`
URL returns 403). The only viable path is a 3-stage filter against the
master CSV dumps:

    Stage 1: dockets-YYYY-MM-DD.csv.bz2 (4.7 GB compressed)
             - stream, filter rows where court_id ∈ STATE_COURTS,
             - persist {docket_id: (court_id, date_filed, case_name)}.

    Stage 2: opinion-clusters-YYYY-MM-DD.csv.bz2 (2.3 GB)
             - stream, filter rows where docket_id ∈ stage-1 set,
             - persist {cluster_id: docket_id}.

    Stage 3: opinions-YYYY-MM-DD.csv.bz2 (50 GB)
             - stream, filter rows where cluster_id ∈ stage-2 set,
             - join back to docket metadata,
             - INSERT into case_documents (opinion_id, court_id,
               full_text_plain, case_name, date_filed, source='courtlistener').

Each stage caches its output so stages are independently resumable. Stage 3
is the expensive one; do not start it until stages 1+2 show a sane scope.

`outcome_for` is left NULL on every inserted row — S22.5 handles outcome
detection separately so a failure in this script doesn't poison labels.

Usage:
    POSTGRES_PASSWORD=... .venv/bin/python scripts/ingest_state_bulks.py \
        --stage dockets|clusters|opinions|all \
        [--data-dir /tmp/state-ingest]
"""
from __future__ import annotations

import argparse
import bz2
import csv
import json
import os
import sys
import time

# Some CL bulk rows (opinions with embedded HTML, very long case names) exceed
# Python's default per-field CSV limit. Bump to the platform max so neither
# the clusters nor opinions stage chokes mid-stream.
csv.field_size_limit(sys.maxsize)
from pathlib import Path
from urllib.request import urlretrieve

import psycopg2
from psycopg2.extras import execute_values

# Latest bulk dump as of 2026-03-31. Bump the date for newer runs.
BULK_DATE = "2026-03-31"
S3_BASE = "https://com-courtlistener-storage.s3-us-west-2.amazonaws.com/bulk-data"

STATE_COURTS: frozenset[str] = frozenset({
    # California
    "cal",            # Supreme Court of California
    "calctapp",       # California Courts of Appeal
    "calag",          # California Attorney General opinions
    # New Jersey
    "nj",             # New Jersey Supreme Court
    "njsuperctappdiv",  # NJ Superior Court, Appellate Division
    "njtaxct",        # NJ Tax Court
})


def download_if_missing(name: str, dest: Path) -> Path:
    """Download a bulk file if absent. Returns the local path."""
    url = f"{S3_BASE}/{name}-{BULK_DATE}.csv.bz2"
    out = dest / f"{name}-{BULK_DATE}.csv.bz2"
    if out.exists():
        print(f"  [{name}] already on disk: {out} ({out.stat().st_size/1024/1024:.0f} MB)")
        return out
    print(f"  [{name}] downloading from {url} …")
    started = time.time()
    urlretrieve(url, out)
    mb = out.stat().st_size / 1024 / 1024
    print(f"  [{name}] downloaded {mb:.0f} MB in {time.time()-started:.0f}s")
    return out


def _index_columns(header: list[str], wanted: list[str]) -> dict[str, int]:
    out = {}
    for w in wanted:
        try:
            out[w] = header.index(w)
        except ValueError:
            raise SystemExit(f"expected column {w!r} not in CSV header")
    return out


def stage_dockets(path: Path, out_path: Path) -> dict[str, dict]:
    """Stream dockets, keep only state-court rows. Returns docket_id -> meta."""
    print(f"\n=== stage 1: filtering dockets for {len(STATE_COURTS)} state courts ===")
    state: dict[str, dict] = {}
    counts: dict[str, int] = {}
    seen = 0
    started = time.time()
    with bz2.open(path, "rt", newline="") as f:
        reader = csv.reader(f)
        header = next(reader)
        idx = _index_columns(header, ["id", "court_id", "date_filed", "case_name"])
        for row in reader:
            seen += 1
            try:
                cid = row[idx["court_id"]]
            except IndexError:
                continue
            if cid in STATE_COURTS:
                docket_id = row[idx["id"]]
                state[docket_id] = {
                    "court_id": cid,
                    "date_filed": row[idx["date_filed"]] or None,
                    "case_name": row[idx["case_name"]] or "",
                }
                counts[cid] = counts.get(cid, 0) + 1
            if seen % 1_000_000 == 0:
                rate = seen / max(1e-6, time.time() - started)
                print(f"    scanned {seen:,} dockets ({rate:,.0f}/s) -> {len(state):,} state hits")
    out_path.write_text(json.dumps(state))
    print(f"  -> {len(state):,} state-court dockets")
    for cid in sorted(counts):
        print(f"     {cid:18s} {counts[cid]:>7,}")
    print(f"  wrote {out_path}")
    return state


def stage_clusters(path: Path, state_dockets: dict[str, dict], out_path: Path) -> dict[str, str]:
    """Stream clusters, keep ones whose docket_id is in state set. Returns cluster_id -> docket_id."""
    print(f"\n=== stage 2: filtering clusters by {len(state_dockets):,} state dockets ===")
    docket_set = set(state_dockets.keys())
    state_clusters: dict[str, str] = {}
    seen = 0
    started = time.time()
    with bz2.open(path, "rt", newline="") as f:
        reader = csv.reader(f)
        header = next(reader)
        idx = _index_columns(header, ["id", "docket_id"])
        for row in reader:
            seen += 1
            try:
                did = row[idx["docket_id"]]
            except IndexError:
                continue
            if did in docket_set:
                state_clusters[row[idx["id"]]] = did
            if seen % 1_000_000 == 0:
                rate = seen / max(1e-6, time.time() - started)
                print(f"    scanned {seen:,} clusters ({rate:,.0f}/s) -> {len(state_clusters):,} state hits")
    out_path.write_text(json.dumps(state_clusters))
    print(f"  -> {len(state_clusters):,} state-court clusters")
    print(f"  wrote {out_path}")
    return state_clusters


def stage_opinions(
    path: Path,
    state_clusters: dict[str, str],
    state_dockets: dict[str, dict],
    batch: int,
    dry_run: bool,
) -> int:
    """Stream opinions, keep ones whose cluster_id is a state cluster, insert."""
    print(f"\n=== stage 3: streaming opinions, filtering by "
          f"{len(state_clusters):,} state clusters ===")
    if not state_clusters:
        print("  no state clusters -> nothing to do.")
        return 0

    cluster_set = set(state_clusters.keys())

    if not dry_run:
        conn = psycopg2.connect(
            host="127.0.0.1",
            port=int(os.environ.get("PGPORT", "5454")),
            dbname=os.environ.get("PGDATABASE", "judicialpredict_dev"),
            user=os.environ.get("PGUSER", "judicialpredict"),
            password=os.environ["POSTGRES_PASSWORD"],
        )
        conn.autocommit = False
        cur = conn.cursor()

    inserted = 0
    seen = 0
    started = time.time()
    buf: list[tuple] = []
    SQL = (
        "INSERT INTO case_documents "
        "(opinion_id, court_id, full_text_plain, case_name, date_filed, source) "
        "VALUES %s ON CONFLICT (opinion_id) DO NOTHING"
    )

    def flush():
        nonlocal buf, inserted
        if not buf or dry_run:
            buf.clear()
            return
        execute_values(cur, SQL, buf, page_size=batch)
        inserted += cur.rowcount if cur.rowcount > 0 else 0
        conn.commit()
        buf.clear()

    # CL bulk CSVs occasionally have rows where an HTML field's embedded
    # quote breaks alignment. Skip those rather than fail-stop the whole run.
    malformed = 0
    matched = 0
    file_size = path.stat().st_size
    # Open the raw bz2 file so we can track *compressed* bytes consumed.
    # bz2.open() / BZ2File.tell() returns the uncompressed offset, which
    # makes ETA against the 51 GB compressed file meaningless.
    raw = open(path, "rb")
    bzf = bz2.BZ2File(raw, mode="rb")
    import io
    f = io.TextIOWrapper(bzf, newline="")
    try:
        # CourtListener's opinions bulk is PostgreSQL `COPY ... CSV` output:
        # embedded quotes inside the long HTML/text columns are escaped as `\"`
        # (backslash), NOT as `""` (CSV double-quote). The default csv.reader
        # treats `\"` as an unescaped quote and breaks out of the quoted-field
        # state mid-row — every multi-line row then comes back with the wrong
        # column count and gets marked malformed. Setting escapechar + disabling
        # doublequote matches the actual dump format.
        reader = csv.reader(f, escapechar="\\", doublequote=False)
        header = next(reader)
        idx = _index_columns(header, ["id", "plain_text", "cluster_id"])
        n_cols = len(header)
        for row in reader:
            seen += 1
            if seen % 100_000 == 0:
                elapsed = time.time() - started
                rate = seen / max(1e-6, elapsed)
                try:
                    pos = raw.tell()
                    pct = 100.0 * pos / file_size
                except Exception:
                    pct = 0.0
                print(f"    [scan] seen={seen:,} ({rate:,.0f}/s) "
                      f"matched={matched:,} inserted={inserted:,} "
                      f"malformed={malformed:,} bz2={pct:.1f}% "
                      f"elapsed={elapsed:.0f}s",
                      flush=True)
            # Hard alignment check first — a misaligned row has the wrong
            # column count, and trying to parse it is just guesswork.
            if len(row) != n_cols:
                malformed += 1
                continue
            cluster_id = row[idx["cluster_id"]]
            if cluster_id not in cluster_set:
                continue
            matched += 1
            text = row[idx["plain_text"]] or ""
            if len(text.strip()) < 50:
                continue  # case_documents.full_text_plain is NOT NULL + meaningful
            try:
                opinion_id = int(row[idx["id"]])
            except (ValueError, TypeError):
                malformed += 1
                continue
            docket_id = state_clusters[cluster_id]
            meta = state_dockets.get(docket_id, {})
            buf.append((
                opinion_id,
                meta.get("court_id"),
                text,
                meta.get("case_name") or "",
                meta.get("date_filed"),
                "courtlistener",
            ))
            if len(buf) >= batch:
                flush()
                rate = seen / max(1e-6, time.time() - started)
                print(f"    [flush] seen={seen:,} ({rate:,.0f}/s) inserted={inserted:,}", flush=True)
        flush()
    finally:
        try:
            f.close()
        except Exception:
            pass
        try:
            bzf.close()
        except Exception:
            pass
        try:
            raw.close()
        except Exception:
            pass
    if not dry_run:
        cur.close()
        conn.close()
    print(f"\n  scanned {seen:,} opinions total")
    print(f"  malformed rows skipped:    {malformed:,}")
    print(f"  inserted (new state rows): {inserted:,}")
    return inserted


def main(stage: str, data_dir: Path, dry_run: bool, batch: int) -> None:
    data_dir.mkdir(parents=True, exist_ok=True)
    dockets_path = data_dir / f"dockets-{BULK_DATE}.csv.bz2"
    clusters_path = data_dir / f"opinion-clusters-{BULK_DATE}.csv.bz2"
    opinions_path = data_dir / f"opinions-{BULK_DATE}.csv.bz2"
    state_dockets_json = data_dir / "state_dockets.json"
    state_clusters_json = data_dir / "state_clusters.json"

    if stage in ("dockets", "all"):
        download_if_missing("dockets", data_dir)
        stage_dockets(dockets_path, state_dockets_json)

    if stage in ("clusters", "all"):
        if not state_dockets_json.exists():
            raise SystemExit("run --stage dockets first")
        state_dockets = json.loads(state_dockets_json.read_text())
        download_if_missing("opinion-clusters", data_dir)
        stage_clusters(clusters_path, state_dockets, state_clusters_json)

    if stage in ("opinions", "all"):
        if not state_clusters_json.exists():
            raise SystemExit("run --stage clusters first")
        state_dockets = json.loads(state_dockets_json.read_text())
        state_clusters = json.loads(state_clusters_json.read_text())
        download_if_missing("opinions", data_dir)
        stage_opinions(opinions_path, state_clusters, state_dockets, batch, dry_run)


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--stage", choices=["dockets", "clusters", "opinions", "all"], required=True)
    p.add_argument("--data-dir", type=Path, default=Path("/tmp/state-ingest"))
    p.add_argument("--dry-run", action="store_true",
                   help="opinions stage: stream + filter but don't INSERT")
    p.add_argument("--batch", type=int, default=500)
    a = p.parse_args()
    main(a.stage, a.data_dir, a.dry_run, a.batch)
