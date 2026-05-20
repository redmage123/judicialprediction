"""
S21.4 — shared legal-BERT sentence-embedding encoder.

Single source of truth for turning opinion text into a 768-dim vector, so the
training-build path (`scripts/build_legal_embeddings.py`) and the inference
path (`predict.py`) embed identically — same model, same pooling, same
truncation. Any drift here silently degrades served predictions, so both
callers MUST go through `embed_texts`.

Model: `nlpaueb/legal-bert-base-uncased`, mean-pooled over the last hidden
state with the attention mask, first 512 tokens. CPU is fine; the model and
tokenizer are loaded once and cached.
"""
from __future__ import annotations

from functools import lru_cache

import numpy as np

MODEL_NAME = "nlpaueb/legal-bert-base-uncased"
EMBEDDING_DIM = 768
MAX_TOKENS = 512


@lru_cache(maxsize=1)
def _load():
    """Load tokenizer + model once. Heavy import kept lazy."""
    import os

    import torch
    from transformers import AutoModel, AutoTokenizer

    os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")
    tok = AutoTokenizer.from_pretrained(MODEL_NAME)
    mdl = AutoModel.from_pretrained(MODEL_NAME)
    mdl.eval()
    return tok, mdl, torch


def embed_texts(
    texts: list[str], batch_size: int = 16
) -> np.ndarray:
    """
    Embed a list of strings into an (N, 768) float32 array.

    Empty / whitespace-only strings map to a zero vector (matching the
    build-time contract: a missing opinion contributes no signal rather than
    a hallucinated one). Texts are truncated to the first 512 tokens.
    """
    tok, mdl, torch = _load()
    out = np.zeros((len(texts), EMBEDDING_DIM), dtype=np.float32)
    nonempty = [(i, t) for i, t in enumerate(texts) if t and t.strip()]
    for start in range(0, len(nonempty), batch_size):
        chunk = nonempty[start : start + batch_size]
        enc = tok(
            [t for _, t in chunk],
            padding=True,
            truncation=True,
            max_length=MAX_TOKENS,
            return_tensors="pt",
        )
        with torch.no_grad():
            res = mdl(**enc)
        mask = enc["attention_mask"].unsqueeze(-1).float()
        summed = (res.last_hidden_state * mask).sum(1)
        counts = mask.sum(1).clamp(min=1e-9)
        vecs = (summed / counts).cpu().numpy().astype(np.float32)
        for j, (orig_i, _) in enumerate(chunk):
            out[orig_i] = vecs[j]
    return out


def column_names() -> list[str]:
    """emb_000 .. emb_767, matching the corpus embedding columns."""
    return [f"emb_{i:03d}" for i in range(EMBEDDING_DIM)]
