"""
S21 — assemble the final corpus (v17) from v14 + the S21 enrichments.

Combines, on top of v14:
  * S21.2 — LLM procedural-posture labels (data/posture_llm_cache.json),
            applied only to rows still tagged `unknown`.
  * S21.4 — legal-BERT 768-dim embeddings (data/legal_bert_emb.npy),
            REPLACING the S20.5 MiniLM 384-dim emb_* columns.

(S21.3 citation-graph features are deferred — no corpus-wide citation data is
available locally and the CourtListener quota is 125/day.)

All non-embedding, non-posture columns are carried through unchanged.

Usage:
    .venv/bin/python scripts/assemble_corpus_v17.py \
        --in data/real_corpus_v14.parquet \
        --emb data/legal_bert_emb.npy \
        --emb-meta data/legal_bert_emb_meta.json \
        --posture-cache data/posture_llm_cache.json \
        --out data/real_corpus_v17.parquet
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np
import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))
from ml_inference_svc.legal_bert import EMBEDDING_DIM, column_names  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
from procedural_posture import POSTURE_CATEGORIES  # noqa: E402

_VALID = frozenset(POSTURE_CATEGORIES)


def main(
    in_path: Path,
    emb_path: Path,
    emb_meta_path: Path,
    posture_cache: Path,
    out_path: Path,
) -> None:
    df = pd.read_parquet(in_path).reset_index(drop=True)
    n = len(df)

    # ---- S21.4: swap embeddings -------------------------------------------
    emb = np.load(emb_path)
    meta = json.loads(emb_meta_path.read_text())
    if meta.get("done", 0) < n:
        raise SystemExit(
            f"embeddings incomplete: done={meta.get('done')} < rows={n}; "
            "finish build_legal_embeddings.py first"
        )
    # Row alignment: the embedding meta stores opinion_ids in the SAME order
    # as the corpus was read. Verify before trusting positional alignment.
    meta_ids = [int(x) for x in meta["opinion_ids"]]
    corpus_ids = df["_opinion_id"].astype("int64").tolist()
    if meta_ids != corpus_ids:
        raise SystemExit("embedding opinion_id order != corpus order; refusing to misalign")
    if emb.shape != (n, EMBEDDING_DIM):
        raise SystemExit(f"embedding shape {emb.shape} != ({n}, {EMBEDDING_DIM})")

    old_emb_cols = [c for c in df.columns if c.startswith("emb_")]
    df = df.drop(columns=old_emb_cols)
    new_cols = column_names()
    emb_df = pd.DataFrame(emb, columns=new_cols, index=df.index)
    df = pd.concat([df, emb_df], axis=1)
    print(f"S21.4: replaced {len(old_emb_cols)} MiniLM cols with {len(new_cols)} legal-BERT cols")

    # ---- S21.2: apply LLM postures ----------------------------------------
    cache: dict[str, str] = json.loads(posture_cache.read_text()) if posture_cache.exists() else {}
    before_unknown = int((df["procedural_posture"] == "unknown").sum())
    oid = df["_opinion_id"].astype(str)
    applied = 0
    posture = df["procedural_posture"].copy()
    for idx in df.index[df["procedural_posture"] == "unknown"]:
        label = cache.get(oid[idx])
        if label and label in _VALID and label != "unknown":
            posture.at[idx] = label
            applied += 1
    df["procedural_posture"] = posture
    after_unknown = int((df["procedural_posture"] == "unknown").sum())
    print(f"S21.2: applied {applied} LLM postures; unknown {before_unknown} -> {after_unknown}")

    df.to_parquet(out_path, index=False)
    print(f"\nwrote {out_path}  ({len(df)} rows, {len(df.columns)} cols)")
    print("posture distribution:")
    print(df["procedural_posture"].value_counts().to_string())


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--in", dest="in_path", required=True, type=Path)
    p.add_argument("--emb", required=True, type=Path)
    p.add_argument("--emb-meta", required=True, type=Path)
    p.add_argument("--posture-cache", required=True, type=Path)
    p.add_argument("--out", required=True, type=Path)
    a = p.parse_args()
    main(a.in_path, a.emb, a.emb_meta, a.posture_cache, a.out)
