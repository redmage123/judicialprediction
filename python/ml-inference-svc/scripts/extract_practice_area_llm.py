"""
S22.1 — tier-2 LLM practice-area labeler.

Same shape as `extract_posture_llm.py`: batch the head of each opinion text
into a single `claude -p` call asking for a JSON array of
`[{"id": <opinion_id>, "practice_area": "<label>"}]`. Auth comes from the
local Claude Code credentials (`~/.claude`); no metered HTTP API call is
made.

Coverage strategy:
  * Run the tier-1 `classify_practice_area` regex first; cache its result.
  * Only send the tier-1 `unknown` residuals to the LLM. The cache is the
    union, so re-runs resume.

Validation: any LLM answer outside PRACTICE_AREAS is recorded as `unknown`
(prevents a hallucinated label from leaking into the trainer's one-hot).

Usage:
    POSTGRES_PASSWORD=... .venv/bin/python scripts/extract_practice_area_llm.py \
        --corpus data/real_corpus_v18.parquet \
        --source db \
        --cache data/practice_area_cache.json \
        [--batch 10] [--head-chars 3000] [--limit N] [--dry-run]
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent))
from practice_area import (  # noqa: E402
    PRACTICE_AREAS,
    classify_practice_area,
)

# `unknown` is a valid LLM answer (means "I genuinely can't tell"); `other`
# is a real bucket ("it's a substantive case but doesn't fit any named area").
# Both stay in the cache so we don't keep re-asking.
_VALID = frozenset(PRACTICE_AREAS)

_CATEGORY_GUIDE = """\
tax                   — federal/state tax law: deficiency, IRS, Tax Court, Title 26.
civil_rights          — §1983, Title VI/VII/IX, ADA, voting rights, equal protection.
criminal              — federal/state criminal: sentencing, indictment, conviction.
employment            — FLSA, ADEA, NLRA, ERISA, FMLA, wrongful termination, labor.
intellectual_property — patent, copyright, trademark, trade secret.
bankruptcy            — Title 11 / Bankruptcy Code, Chapter 7/11/13, automatic stay.
immigration           — INA, asylum, removal, BIA review, 8 U.S.C.
antitrust             — Sherman/Clayton Act, monopolization, price fixing.
securities            — '33/'34 Act, SEC enforcement, Rule 10b-5, insider trading.
administrative        — APA review of agency action (non-immigration/tax/SEC).
contract              — breach of contract, UCC, specific performance.
tort                  — negligence, products liability, personal injury.
real_property         — foreclosure, eminent domain, takings, quiet title.
family                — custody, divorce, support, adoption.
other                 — substantive case but none of the above fits cleanly.
unknown               — text doesn't let you decide (use sparingly).\
"""

_PROMPT_HEADER = f"""\
You are a legal-docket classifier. For each opinion excerpt below, identify
its dominant SUBSTANTIVE-LAW area — what body of law the case is actually
about — choosing exactly one label from this closed set:

{_CATEGORY_GUIDE}

Rules:
- Pick the dominant area. Procedural posture (motion to dismiss, summary
  judgment, habeas) is NOT the substantive area; look past it. A §1983
  excessive-force case is `civil_rights`, not `criminal`, even if the
  underlying conduct was an arrest.
- Tax Court cases are almost always `tax`. Immigration appeals from the BIA
  are `immigration`. NLRB enforcement is `employment`. SEC enforcement is
  `securities`.
- If the excerpt does not let you decide between two labels, prefer the
  more specific (e.g. `securities` over `administrative`). If you genuinely
  can't tell, answer `unknown` — do not guess.
- Output ONLY a JSON array, no prose, no markdown fences. Each element:
  {{"id": <the integer id given>, "practice_area": "<one label>"}}
- Return exactly one element per excerpt, preserving the given ids.

Opinions:
"""


def build_text_map_db(opinion_ids: list[int]) -> dict[str, str]:
    """opinion_id (str) -> full_text_plain, from case_documents."""
    import psycopg2

    conn = psycopg2.connect(
        host="127.0.0.1",
        port=int(os.environ.get("PGPORT", "5454")),
        dbname=os.environ.get("PGDATABASE", "judicialpredict_dev"),
        user=os.environ.get("PGUSER", "judicialpredict"),
        password=os.environ["POSTGRES_PASSWORD"],
    )
    out: dict[str, str] = {}
    try:
        with conn.cursor() as cur:
            for i in range(0, len(opinion_ids), 1000):
                chunk = opinion_ids[i : i + 1000]
                cur.execute(
                    "SELECT opinion_id, full_text_plain FROM case_documents "
                    "WHERE opinion_id = ANY(%s)",
                    (chunk,),
                )
                for oid, txt in cur.fetchall():
                    out[str(int(oid))] = txt or ""
    finally:
        conn.close()
    return out


def call_claude(prompt: str, timeout_s: int = 180) -> str:
    proc = subprocess.run(
        ["claude", "-p", prompt, "--output-format", "text"],
        capture_output=True,
        text=True,
        timeout=timeout_s,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"claude -p failed ({proc.returncode}): {proc.stderr[:400]}")
    return proc.stdout.strip()


def parse_array(raw: str) -> list[dict]:
    start = raw.find("[")
    end = raw.rfind("]")
    if start == -1 or end == -1 or end <= start:
        raise ValueError(f"no JSON array in output: {raw[:200]!r}")
    return json.loads(raw[start : end + 1])


def label_batch(batch: list[tuple[str, str]], head_chars: int) -> dict[str, str]:
    parts = [_PROMPT_HEADER]
    for oid, text in batch:
        head = " ".join(text[:head_chars].split())  # collapse whitespace
        parts.append(f'\n--- id: {oid} ---\n{head}\n')
    raw = call_claude("".join(parts))
    arr = parse_array(raw)
    result: dict[str, str] = {}
    for item in arr:
        oid = str(item.get("id"))
        area = str(item.get("practice_area", "")).strip().lower()
        result[oid] = area if area in _VALID else "unknown"
    return result


def main(
    corpus: Path,
    cache_path: Path,
    batch: int,
    head_chars: int,
    limit: int | None,
    dry_run: bool,
    source: str,
) -> None:
    df = pd.read_parquet(corpus)

    # Cache is the SINGLE source of truth — it carries both tier-1 (regex)
    # and tier-2 (LLM) labels keyed by opinion_id. Re-runs only ask the LLM
    # about opinions that are still missing or still "unknown".
    cache: dict[str, str] = {}
    if cache_path.exists():
        cache = json.loads(cache_path.read_text())

    ids_all = df["_opinion_id"].astype("int64").tolist()
    text_map: dict[str, str] = {}
    if source == "db":
        text_map = build_text_map_db(ids_all)
    else:
        raise SystemExit("only --source db is supported for S22.1")

    # Tier-1 pass: regex over text, populate cache cheaply for everything not
    # already cached. Then collect tier-2 targets (those still unknown).
    tier1_added = 0
    for oid_int, oid in [(i, str(i)) for i in ids_all]:
        if oid in cache and cache[oid] != "unknown":
            continue
        txt = text_map.get(oid, "")
        pa = classify_practice_area(txt)
        if pa.label != "unknown":
            cache[oid] = pa.label
            tier1_added += 1

    if tier1_added:
        cache_path.write_text(json.dumps(cache, indent=2))

    # Tier-2 candidates: still unknown (or never seen) AND have text.
    todo: list[tuple[str, str]] = []
    for oid_int in ids_all:
        oid = str(oid_int)
        if cache.get(oid) and cache.get(oid) != "unknown":
            continue
        txt = text_map.get(oid)
        if txt and txt.strip():
            todo.append((oid, txt))

    if limit is not None:
        todo = todo[:limit]

    n_rows = len(df)
    tier1_total = sum(1 for v in cache.values() if v != "unknown")
    print(f"corpus rows           : {n_rows}")
    print(f"tier-1 added this run : {tier1_added}")
    print(f"tier-1 labeled (total): {tier1_total} ({100*tier1_total/n_rows:.1f}%)")
    print(f"tier-2 LLM targets    : {len(todo)}  (batch={batch}, "
          f"~{(len(todo)+batch-1)//batch} calls)")
    if dry_run:
        print("--dry-run: no LLM calls made")
        return
    if not todo:
        print("nothing for tier-2 to label.")
        return

    started = time.time()
    n_calls = 0
    for i in range(0, len(todo), batch):
        chunk = todo[i : i + batch]
        try:
            labels = label_batch(chunk, head_chars)
        except Exception as exc:  # noqa: BLE001 — keep going; cache the rest
            print(f"  [batch {i//batch}] ERROR: {exc} — skipping", file=sys.stderr)
            continue
        for oid, _ in chunk:
            cache[oid] = labels.get(oid, "unknown")
        n_calls += 1
        cache_path.write_text(json.dumps(cache, indent=2))
        done = min(i + batch, len(todo))
        rate = done / max(1e-6, time.time() - started)
        print(f"  labeled {done}/{len(todo)}  ({rate:.1f}/s)  call#{n_calls}")

    import collections
    print("\nfinal practice_area distribution:")
    print(collections.Counter(cache.values()))
    print(f"\ncache: {cache_path}  ({len(cache)} opinions)")


if __name__ == "__main__":
    here = Path(__file__).resolve().parent
    default_cache = here.parent / "data" / "practice_area_cache.json"
    p = argparse.ArgumentParser()
    p.add_argument("--corpus", required=True, type=Path)
    p.add_argument("--cache", type=Path, default=default_cache)
    p.add_argument("--source", choices=["db"], default="db")
    p.add_argument("--batch", type=int, default=10)
    p.add_argument("--head-chars", type=int, default=3000)
    p.add_argument("--limit", type=int, default=None)
    p.add_argument("--dry-run", action="store_true")
    a = p.parse_args()
    main(a.corpus, a.cache, a.batch, a.head_chars, a.limit, a.dry_run, a.source)
