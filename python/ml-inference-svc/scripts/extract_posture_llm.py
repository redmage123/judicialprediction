"""
S21.2 — tier-2 LLM procedural-posture fallback.

`procedural_posture.classify_procedural_posture` is a deterministic regex
(tier 1) that leaves ~73% of real-circuit opinions as `unknown`. This script
is the tier-2 fallback the regex docstring promised: it sends the head of each
`unknown` opinion to the `claude` CLI (print mode, `-p`) and asks it to pick
one of the POSTURE_CATEGORIES buckets.

Auth: uses the local Claude Code credentials (`~/.claude`) via the `claude`
binary — NOT the Anthropic HTTP API — so there is no separate API key or
metered per-token spend to manage.

Coverage note: this labels only opinions whose `full_text_plain` is recoverable
locally (from the probe export). Rows without recoverable text stay `unknown`;
re-fetching their text from CourtListener is a separate ingest step.

Design:
  * Opinions are batched (default 10/call) into one prompt that asks for a
    strict JSON array `[{"id": <opinion_id>, "posture": "<bucket>"}]`.
  * Results are cached to `data/posture_llm_cache.json` keyed by opinion_id,
    so re-runs resume and never re-pay for an already-labeled opinion.
  * Any posture outside POSTURE_CATEGORIES (or a missing id) is recorded as
    `unknown` so a hallucinated label can never leak into the trainer's
    one-hot enum.

Usage:
    .venv/bin/python scripts/extract_posture_llm.py \
        --corpus data/real_corpus_v14.parquet \
        --probe ../../.tmp/probe_v5.cleaned.json \
        [--batch 10] [--head-chars 2200] [--limit N] [--dry-run]
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent))
from procedural_posture import POSTURE_CATEGORIES  # noqa: E402

# Valid answers the LLM may return. `unknown` is allowed — the model is told
# to use it when no posture is identifiable rather than guessing.
_VALID = frozenset(POSTURE_CATEGORIES)

_CATEGORY_GUIDE = """\
cert_petition    — Supreme Court review on a petition for writ of certiorari.
en_banc          — full-court (en banc) rehearing in a court of appeals.
summary_judgment — appeal centers on a Rule 56 summary-judgment ruling.
motion_dismiss   — appeal centers on a Rule 12(b)(6)-style motion to dismiss.
daubert          — challenge to expert testimony admissibility (Daubert).
habeas           — habeas corpus petition (28 U.S.C. 2241/2254/2255).
direct_appeal    — ordinary direct appeal of a trial-court judgment.
agency_review    — appellate review of an administrative/agency decision
                   (e.g. BIA immigration, Tax Court, NLRB, SSA).
rehearing        — panel rehearing (not en banc).
unknown          — none of the above can be determined from the text.\
"""

_PROMPT_HEADER = f"""\
You are a legal-docket classifier. For each opinion excerpt below, identify its
single dominant PROCEDURAL POSTURE — what kind of proceeding produced the
opinion — choosing exactly one label from this closed set:

{_CATEGORY_GUIDE}

Rules:
- Choose the top-level posture. A cert petition that internally discusses a
  motion to dismiss is `cert_petition`. Immigration/Tax-Court/agency appeals are
  `agency_review`, not `direct_appeal`.
- If the excerpt does not let you decide, answer `unknown`. Do not guess.
- Output ONLY a JSON array, no prose, no markdown fences. Each element:
  {{"id": <the integer id given>, "posture": "<one label>"}}
- Return exactly one element per excerpt, preserving the given ids.

Opinions:
"""


def build_text_map(probe_path: Path) -> dict[str, str]:
    """opinion_id (str) -> full_text_plain, from the probe export."""
    data = json.loads(probe_path.read_text())
    out: dict[str, str] = {}
    for rec in data:
        oid = rec.get("opinion_id")
        if oid is not None:
            out[str(oid)] = rec.get("full_text_plain") or ""
    return out


def build_text_map_db(opinion_ids: list[int]) -> dict[str, str]:
    """
    opinion_id (str) -> full_text_plain, from the local case_documents table.
    Far higher coverage than the probe export (~5,933/5,937 vs 631). Needs
    POSTGRES_PASSWORD in the environment.
    """
    import os

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
    """Invoke the local `claude` CLI in print mode. Returns raw stdout."""
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
    """Extract the first JSON array from the model output, tolerantly."""
    start = raw.find("[")
    end = raw.rfind("]")
    if start == -1 or end == -1 or end <= start:
        raise ValueError(f"no JSON array in output: {raw[:200]!r}")
    return json.loads(raw[start : end + 1])


def label_batch(
    batch: list[tuple[str, str]], head_chars: int
) -> dict[str, str]:
    """Send one batch of (opinion_id, text) and return {id: posture}."""
    parts = [_PROMPT_HEADER]
    for oid, text in batch:
        head = " ".join(text[:head_chars].split())  # collapse whitespace
        parts.append(f'\n--- id: {oid} ---\n{head}\n')
    raw = call_claude("".join(parts))
    arr = parse_array(raw)
    result: dict[str, str] = {}
    for item in arr:
        oid = str(item.get("id"))
        posture = str(item.get("posture", "")).strip().lower()
        result[oid] = posture if posture in _VALID else "unknown"
    return result


def main(
    corpus: Path,
    probe: Path,
    cache_path: Path,
    batch: int,
    head_chars: int,
    limit: int | None,
    dry_run: bool,
    source: str = "probe",
) -> None:
    df = pd.read_parquet(corpus)
    unknown = df[df["procedural_posture"] == "unknown"]

    if source == "db":
        ids = unknown["_opinion_id"].astype("int64").tolist()
        text_map = build_text_map_db(ids)
    else:
        text_map = build_text_map(probe)

    targets: list[tuple[str, str]] = []
    for oid in unknown["_opinion_id"].astype(str):
        txt = text_map.get(oid)
        if txt and txt.strip():
            targets.append((oid, txt))

    cache: dict[str, str] = {}
    if cache_path.exists():
        cache = json.loads(cache_path.read_text())
    todo = [(oid, txt) for oid, txt in targets if oid not in cache]
    if limit is not None:
        todo = todo[:limit]

    print(f"unknown rows           : {len(unknown)}")
    print(f"  recoverable text     : {len(targets)}")
    print(f"  already cached       : {len(targets) - len([t for t in targets if t[0] not in cache])}")
    print(f"  to label this run    : {len(todo)}  (batch={batch}, ~{(len(todo)+batch-1)//batch} calls)")
    if dry_run:
        print("--dry-run: no LLM calls made")
        return
    if not todo:
        print("nothing to label.")
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
        cache_path.write_text(json.dumps(cache, indent=2))  # checkpoint each batch
        done = min(i + batch, len(todo))
        rate = done / max(1e-6, time.time() - started)
        print(f"  labeled {done}/{len(todo)}  ({rate:.1f}/s)  call#{n_calls}")

    # Summary of what we assigned this run.
    assigned = [cache[oid] for oid, _ in todo if oid in cache]
    dist = pd.Series(assigned).value_counts()
    print("\nposture distribution (this run):")
    print(dist.to_string())
    print(f"\ncache written: {cache_path}  ({len(cache)} opinions)")


if __name__ == "__main__":
    here = Path(__file__).resolve().parent
    default_cache = here.parent / "data" / "posture_llm_cache.json"
    p = argparse.ArgumentParser()
    p.add_argument("--corpus", required=True, type=Path)
    p.add_argument("--probe", type=Path, default=None,
                   help="probe JSON export (required only for --source probe)")
    p.add_argument("--cache", type=Path, default=default_cache)
    p.add_argument("--batch", type=int, default=10)
    p.add_argument("--head-chars", type=int, default=2200)
    p.add_argument("--limit", type=int, default=None)
    p.add_argument("--source", choices=["probe", "db"], default="probe",
                   help="text source for unknown opinions (db = case_documents, far higher coverage)")
    p.add_argument("--dry-run", action="store_true")
    a = p.parse_args()
    main(a.corpus, a.probe, a.cache, a.batch, a.head_chars, a.limit, a.dry_run, a.source)
