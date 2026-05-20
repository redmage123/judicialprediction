"""
S21.2 — fold the tier-2 LLM postures into a new corpus version.

The original v14 input (5,937 opinions with full text) is not present locally,
so we do NOT rebuild the corpus from scratch (that would regress to the ~5,109
opinions in the probe export and lose v14's citation features + embeddings).
Instead we augment v14 IN PLACE: for every row still tagged `unknown` whose
opinion_id was labeled by `extract_posture_llm.py`, overwrite
`procedural_posture` with the LLM label. Every other column — citations,
embeddings, party types, outcomes — is carried through byte-for-byte.

Output: data/real_corpus_v15.parquet.

Usage:
    .venv/bin/python scripts/apply_posture_llm.py \
        --in data/real_corpus_v14.parquet \
        --cache data/posture_llm_cache.json \
        --out data/real_corpus_v15.parquet
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent))
from procedural_posture import POSTURE_CATEGORIES  # noqa: E402

_VALID = frozenset(POSTURE_CATEGORIES)


def main(in_path: Path, cache_path: Path, out_path: Path) -> None:
    df = pd.read_parquet(in_path)
    cache: dict[str, str] = json.loads(cache_path.read_text())

    before = df["procedural_posture"].value_counts(dropna=False)

    oid = df["_opinion_id"].astype(str)
    is_unknown = df["procedural_posture"] == "unknown"
    n_applied = 0
    n_still_unknown_after_llm = 0
    new_posture = df["procedural_posture"].copy()
    for idx in df.index[is_unknown]:
        label = cache.get(oid[idx])
        if label is None:
            continue
        if label not in _VALID:
            label = "unknown"
        if label != "unknown":
            new_posture.at[idx] = label
            n_applied += 1
        else:
            n_still_unknown_after_llm += 1
    df["procedural_posture"] = new_posture

    df.to_parquet(out_path, index=False)

    after = df["procedural_posture"].value_counts(dropna=False)
    print("=== procedural_posture: before -> after ===")
    cats = sorted(set(before.index) | set(after.index))
    for c in cats:
        print(f"  {c:18s} {int(before.get(c,0)):5d} -> {int(after.get(c,0)):5d}")
    print(f"\nLLM labels applied (non-unknown): {n_applied}")
    print(f"LLM saw but kept unknown        : {n_still_unknown_after_llm}")
    unk_after = int(after.get("unknown", 0))
    print(f"remaining unknown               : {unk_after} ({100*unk_after/len(df):.1f}%)")
    print(f"\nwrote {out_path}  ({len(df)} rows, {len(df.columns)} cols)")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--in", dest="in_path", required=True, type=Path)
    p.add_argument("--cache", required=True, type=Path)
    p.add_argument("--out", required=True, type=Path)
    a = p.parse_args()
    main(a.in_path, a.cache, a.out)
