"""
S22.2 — load the CourtListener citation-map bulk dump into
`case_document_citations`.

Stream-decompresses a bz2 CSV (citing_opinion_id, cited_opinion_id[, depth]),
keeps only edges where BOTH endpoints exist in our local `case_documents`,
and bulk-inserts the survivors with `ON CONFLICT DO NOTHING` (the PK is
`(citing_opinion_id, cited_opinion_id)`, so a partial earlier load resumes
cleanly).

No CourtListener REST quota is consumed — this is a one-time download from
the public S3 bucket. The latest dump as of 2026-05-20 is ≈500 MB compressed,
which decompresses to roughly the same row count CL claims in the bulk-data
docs (tens of millions of citation edges). The vast majority will be filtered
out (we only keep edges where both opinions are in our 40,802-row corpus),
so the final insert volume is modest.

Usage:
    POSTGRES_PASSWORD=... .venv/bin/python scripts/load_citations_bulk.py \
        --bulk /tmp/citation-map.csv.bz2 [--batch 10000] [--limit N]
"""
from __future__ import annotations

import argparse
import bz2
import csv
import os
import time
from pathlib import Path

import psycopg2
from psycopg2.extras import execute_values


def load_corpus_opinion_ids(conn) -> set[int]:
    with conn.cursor() as cur:
        cur.execute("SELECT opinion_id FROM case_documents")
        ids = {int(r[0]) for r in cur.fetchall()}
    return ids


def stream_filter(
    bulk_path: Path,
    corpus_ids: set[int],
    limit: int | None,
):
    """
    Yield (citing, cited) tuples where both endpoints are in `corpus_ids`.
    Skips self-loops (the FK + PK accept them but the table has a CHECK
    constraint forbidding them — we'd hit a constraint error mid-batch).
    """
    with bz2.open(bulk_path, "rt", newline="") as f:
        reader = csv.reader(f)
        header = next(reader, None)
        # CL bulk format: citing_opinion_id, cited_opinion_id[, depth]. Be
        # tolerant — locate the two id columns by name when a header is present.
        if header and header[0].strip().isalpha():
            try:
                ci = header.index("citing_opinion_id")
                cj = header.index("cited_opinion_id")
            except ValueError:
                ci, cj = 0, 1
        else:
            ci, cj = 0, 1
            # Header wasn't a header — first data row, replay it.
            if header is not None:
                try:
                    yield int(header[ci]), int(header[cj])
                except (ValueError, IndexError):
                    pass

        emitted = 0
        for row in reader:
            try:
                a = int(row[ci]); b = int(row[cj])
            except (ValueError, IndexError):
                continue
            if a == b:
                continue
            if a in corpus_ids and b in corpus_ids:
                yield a, b
                emitted += 1
                if limit is not None and emitted >= limit:
                    return


def main(bulk_path: Path, batch: int, limit: int | None) -> None:
    conn = psycopg2.connect(
        host="127.0.0.1",
        port=int(os.environ.get("PGPORT", "5454")),
        dbname=os.environ.get("PGDATABASE", "judicialpredict_dev"),
        user=os.environ.get("PGUSER", "judicialpredict"),
        password=os.environ["POSTGRES_PASSWORD"],
    )
    conn.autocommit = False

    print("loading corpus opinion_id set…")
    corpus_ids = load_corpus_opinion_ids(conn)
    print(f"  case_documents has {len(corpus_ids)} opinion_ids")

    started = time.time()
    seen = 0
    inserted = 0
    buf: list[tuple[int, int]] = []
    sql = (
        "INSERT INTO case_document_citations (citing_opinion_id, cited_opinion_id) "
        "VALUES %s ON CONFLICT DO NOTHING"
    )
    with conn.cursor() as cur:
        for edge in stream_filter(bulk_path, corpus_ids, limit):
            buf.append(edge)
            seen += 1
            if len(buf) >= batch:
                execute_values(cur, sql, buf, page_size=batch)
                inserted += cur.rowcount if cur.rowcount > 0 else 0
                conn.commit()
                buf.clear()
                if seen % (batch * 10) == 0:
                    rate = seen / max(1e-6, time.time() - started)
                    print(f"  filtered {seen:,} edges ({rate:,.0f}/s)")
        if buf:
            execute_values(cur, sql, buf, page_size=batch)
            inserted += cur.rowcount if cur.rowcount > 0 else 0
            conn.commit()

    with conn.cursor() as cur:
        cur.execute("SELECT count(*) FROM case_document_citations")
        total = cur.fetchone()[0]
    conn.close()

    print(f"\nedges considered (in-corpus, post-filter): {seen:,}")
    print(f"new edges inserted this run              : {inserted:,}")
    print(f"case_document_citations total now        : {total:,}")
    print(f"elapsed: {time.time() - started:.1f}s")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--bulk", required=True, type=Path,
                   help="path to citation-map-*.csv.bz2")
    p.add_argument("--batch", type=int, default=10000)
    p.add_argument("--limit", type=int, default=None,
                   help="cap on emitted in-corpus edges (testing)")
    a = p.parse_args()
    main(a.bulk, a.batch, a.limit)
