"""
S22 — assemble corpus v19 from v18 + S22.1 practice_area.

v18 already has the legal-BERT embeddings (S21.4) and the full LLM posture
coverage (S21.2). v19 layers in the S22.1 substantive-law feature.

S22.3 (cited pet/resp-favored counts) is intentionally NOT included on the
federal-only corpus: only 12 / 5,937 rows have temporal-safe cited-with-
outcome edges, so the columns would be ~99.8% zeros. The pipeline is in
place (`load_citations_bulk.py` + `compute_cited_features.py`); re-run it
once S22.4 has expanded the corpus with the NJ/CA state opinions.

Usage:
    .venv/bin/python scripts/assemble_corpus_v19.py \
        --in data/real_corpus_v18.parquet \
        --practice-area-cache data/practice_area_cache.json \
        --out data/real_corpus_v19.parquet
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent))
from practice_area import PRACTICE_AREAS  # noqa: E402

_VALID = frozenset(PRACTICE_AREAS)


def main(in_path: Path, cache_path: Path, out_path: Path) -> None:
    df = pd.read_parquet(in_path).reset_index(drop=True)
    cache: dict[str, str] = json.loads(cache_path.read_text())

    oid = df["_opinion_id"].astype(str)
    n_applied = 0
    n_unknown = 0
    practice = []
    for idx in df.index:
        label = cache.get(oid[idx], "unknown")
        if label not in _VALID:
            label = "unknown"
        practice.append(label)
        if label == "unknown":
            n_unknown += 1
        else:
            n_applied += 1
    df["practice_area"] = practice

    df.to_parquet(out_path, index=False)
    print(f"S22.1: applied {n_applied} practice_area labels; "
          f"{n_unknown} unknown ({100*n_unknown/len(df):.1f}%)")
    print(f"\nwrote {out_path}  ({len(df)} rows, {len(df.columns)} cols)")
    print("\npractice_area distribution:")
    print(df["practice_area"].value_counts().to_string())


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--in", dest="in_path", required=True, type=Path)
    p.add_argument("--practice-area-cache", required=True, type=Path)
    p.add_argument("--out", required=True, type=Path)
    a = p.parse_args()
    main(a.in_path, a.practice_area_cache, a.out)
