"""
S21.4 — legal-BERT opinion embeddings (768-dim), computed from the DB.

Swaps the S20.5 MiniLM embedding (384-dim, general-domain) for
`nlpaueb/legal-bert-base-uncased` (768-dim, pre-trained on legal corpora).
Embeddings are mean-pooled over the last hidden state (attention-masked) on
the first 512 tokens of `full_text_plain`.

Source of truth is the local `case_documents` Postgres table (full text for
5,933/5,937 v14 opinions), NOT the partial probe export — so coverage is
effectively complete. The 4 opinions absent from the DB get a zero vector.

This script ONLY computes embeddings and writes a sidecar:
    data/legal_bert_emb.npy        (N x 768 float32, row-aligned to v14)
    data/legal_bert_emb_meta.json  ({"opinion_ids": [...], "done": K, "dim": 768})
It checkpoints every --checkpoint rows and is resumable: re-running picks up
from `done`. `assemble_corpus_v17.py` consumes the sidecar to build the parquet.

Usage:
    POSTGRES_PASSWORD=... .venv/bin/python scripts/build_legal_embeddings.py \
        --corpus data/real_corpus_v14.parquet [--batch 16] [--checkpoint 400]
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path

import numpy as np
import pandas as pd
import psycopg2

MODEL_NAME = "nlpaueb/legal-bert-base-uncased"
EMB_DIM = 768
MAX_TOKENS = 512


def fetch_texts(opinion_ids: list[int]) -> dict[int, str]:
    """opinion_id -> full_text_plain from case_documents (chunked IN query)."""
    conn = psycopg2.connect(
        host="127.0.0.1",
        port=int(os.environ.get("PGPORT", "5454")),
        dbname=os.environ.get("PGDATABASE", "judicialpredict_dev"),
        user=os.environ.get("PGUSER", "judicialpredict"),
        password=os.environ["POSTGRES_PASSWORD"],
    )
    out: dict[int, str] = {}
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
                    out[int(oid)] = txt or ""
    finally:
        conn.close()
    return out


def main(corpus: Path, batch: int, checkpoint: int) -> None:
    import torch
    from transformers import AutoModel, AutoTokenizer

    df = pd.read_parquet(corpus)
    opinion_ids = df["_opinion_id"].astype("int64").tolist()
    n = len(opinion_ids)

    here = Path(__file__).resolve().parent
    data_dir = here.parent / "data"
    emb_path = data_dir / "legal_bert_emb.npy"
    meta_path = data_dir / "legal_bert_emb_meta.json"

    # Resume support.
    if emb_path.exists() and meta_path.exists():
        emb = np.load(emb_path)
        meta = json.loads(meta_path.read_text())
        done = int(meta.get("done", 0))
        if emb.shape != (n, EMB_DIM):
            emb = np.zeros((n, EMB_DIM), dtype=np.float32)
            done = 0
    else:
        emb = np.zeros((n, EMB_DIM), dtype=np.float32)
        done = 0

    if done >= n:
        print(f"already complete: {done}/{n}")
        return

    print(f"fetching text for {n} opinions from DB…")
    texts_by_id = fetch_texts(opinion_ids)
    missing = sum(1 for oid in opinion_ids if not texts_by_id.get(oid, "").strip())
    print(f"  text present: {n - missing}/{n}  (missing -> zero vector)")

    print(f"loading {MODEL_NAME}…")
    tok = AutoTokenizer.from_pretrained(MODEL_NAME)
    mdl = AutoModel.from_pretrained(MODEL_NAME)
    mdl.eval()
    torch.set_num_threads(max(1, os.cpu_count() or 1))

    print(f"embedding from row {done}/{n} (batch={batch})…")
    for start in range(done, n, batch):
        end = min(start + batch, n)
        batch_texts = [texts_by_id.get(opinion_ids[r], "") or "" for r in range(start, end)]
        # Rows with empty text stay as the pre-zeroed vector.
        idx_nonempty = [k for k, t in enumerate(batch_texts) if t.strip()]
        if idx_nonempty:
            enc = tok(
                [batch_texts[k] for k in idx_nonempty],
                padding=True,
                truncation=True,
                max_length=MAX_TOKENS,
                return_tensors="pt",
            )
            with torch.no_grad():
                out = mdl(**enc)
            mask = enc["attention_mask"].unsqueeze(-1).float()
            summed = (out.last_hidden_state * mask).sum(1)
            counts = mask.sum(1).clamp(min=1e-9)
            vecs = (summed / counts).cpu().numpy().astype(np.float32)
            for j, k in enumerate(idx_nonempty):
                emb[start + k] = vecs[j]
        done = end
        if done % checkpoint < batch or done == n:
            np.save(emb_path, emb)
            meta_path.write_text(json.dumps({"opinion_ids": opinion_ids, "done": done, "dim": EMB_DIM}))
            print(f"  checkpoint {done}/{n}")

    np.save(emb_path, emb)
    meta_path.write_text(json.dumps({"opinion_ids": opinion_ids, "done": n, "dim": EMB_DIM}))
    print(f"done: {n} embeddings -> {emb_path}")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--corpus", required=True, type=Path)
    p.add_argument("--batch", type=int, default=16)
    p.add_argument("--checkpoint", type=int, default=400)
    a = p.parse_args()
    main(a.corpus, a.batch, a.checkpoint)
