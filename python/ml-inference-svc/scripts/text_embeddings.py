"""
Opinion text embeddings — Sprint 20.5.

Encodes each opinion's `full_text_plain` into a 384-dim dense vector
using `sentence-transformers/all-MiniLM-L6-v2`. Same model the
AI Elevate engineering RAG uses, so the dependency footprint is the
one we already accept elsewhere on the box.

Why all-MiniLM-L6-v2 and not legal-bert-base-uncased:
  * MiniLM is 23M params vs BERT-base's 110M — ~5x faster on CPU.
  * MiniLM is already trained for sentence-similarity (mean-pooled
    CLS-replacement); legal-bert is a fill-mask model that needs a
    sentence-transformers wrapper to mean-pool.
  * The corpus is f3d/SCOTUS appellate prose; specialty legal-BERT
    helps most on terms-of-art-heavy contract / statutory text. For
    appellate dispositions the general-purpose model captures
    "argument was rejected" / "agency was affirmed" patterns fine.
  * If the v14 retrain plateaus and embeddings are the obvious culprit,
    swap the model id and rebuild — the rest of the pipeline doesn't
    care.

Each opinion is truncated to the model's max_seq_length (256 tokens
for MiniLM) at the head — most appellate opinions state the
disposition + reasoning in the first ~500 chars. Tail truncation
loses the syllabus / signature lines which are mostly procedural.
"""
from __future__ import annotations

import os
from typing import Iterable, Sequence

import numpy as np

# Lazy import — the module is large (torch, transformers) and we don't
# want to pay the cost on python ./build_real_corpus.py --help.
_MODEL = None


def _load_model():
    global _MODEL  # noqa: PLW0603
    if _MODEL is None:
        # Quiet HF progress bars
        os.environ.setdefault("TRANSFORMERS_NO_ADVISORY_WARNINGS", "1")
        os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")
        from sentence_transformers import SentenceTransformer
        _MODEL = SentenceTransformer("all-MiniLM-L6-v2")
    return _MODEL


EMBEDDING_DIM = 384


def embed_opinions(
    texts: Sequence[str | None],
    batch_size: int = 32,
    show_progress: bool = True,
) -> np.ndarray:
    """
    Embed a batch of opinion texts. Returns float32 ndarray of shape
    (len(texts), EMBEDDING_DIM). Empty / None texts get a zero vector
    (the model would emit a small constant for the empty string; zero
    is a cleaner neutral that the downstream model can ignore).
    """
    model = _load_model()
    # Replace None / empty with a single placeholder so the encoder
    # doesn't crash; we overwrite those rows with zeros at the end.
    safe_texts = [t if (t and t.strip()) else "PLACEHOLDER" for t in texts]
    vectors = model.encode(
        safe_texts,
        batch_size=batch_size,
        show_progress_bar=show_progress,
        convert_to_numpy=True,
    ).astype(np.float32)
    for i, t in enumerate(texts):
        if not (t and t.strip()):
            vectors[i] = 0.0
    return vectors


def column_names() -> list[str]:
    """Stable column names for the 384 embedding dims. The trainer
    relies on the prefix to identify embedding columns."""
    return [f"emb_{i:03d}" for i in range(EMBEDDING_DIM)]
