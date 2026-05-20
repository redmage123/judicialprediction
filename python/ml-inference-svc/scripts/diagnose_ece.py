"""
S21.5 diagnostic — decompose the ECE regression between the v14 (MiniLM)
champion and the v17 (legal-BERT) champion.

Reproduces the trainer's exact held-out split (test_size=0.2, seed=42,
stratify=y) for each corpus, loads each champion model from mlruns, and prints
a per-bin reliability table so we can see WHERE calibration drifted
(over/under-confidence, which probability band, sample counts).

Usage:
    PYTHONPATH=src .venv/bin/python scripts/diagnose_ece.py
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

import mlflow
import numpy as np
import pandas as pd
from sklearn.model_selection import train_test_split

sys.path.insert(0, str(Path(__file__).resolve().parent))
import train_first_models as tfm  # noqa: E402
from train_first_models import _resolve_feature_cols, encode_features  # noqa: E402

PROJECT_ROOT = Path(__file__).resolve().parent.parent


def load_model(run_id: str):
    mlflow.set_tracking_uri("file://" + str(PROJECT_ROOT / "mlruns"))
    mlruns = PROJECT_ROOT / "mlruns"
    for exp in mlruns.iterdir():
        outputs = exp / run_id / "outputs"
        if outputs.is_dir():
            for name in os.listdir(outputs):
                if name.startswith("m-"):
                    art = exp / "models" / name / "artifacts"
                    if (art / "MLmodel").is_file():
                        return mlflow.sklearn.load_model(str(art))
    return mlflow.sklearn.load_model(f"runs:/{run_id}/model")


def predict_proba(model, X, courts):
    try:
        return model.predict_proba(X, court_ids=courts)[:, 1]
    except TypeError:
        return model.predict_proba(X)[:, 1]


def reliability(y_true, p, n_bins=10):
    bins = np.linspace(0, 1, n_bins + 1)
    rows = []
    ece = 0.0
    n = len(y_true)
    for lo, hi in zip(bins[:-1], bins[1:]):
        m = (p >= lo) & (p < hi) if hi < 1.0 else (p >= lo) & (p <= hi)
        cnt = int(m.sum())
        if cnt == 0:
            rows.append((lo, hi, 0, float("nan"), float("nan"), 0.0))
            continue
        conf = float(p[m].mean())
        acc = float(y_true[m].mean())
        gap = abs(conf - acc)
        ece += (cnt / n) * gap
        rows.append((lo, hi, cnt, conf, acc, gap))
    return ece, rows


def evaluate(label: str, parquet: str, run_id: str):
    df = pd.read_parquet(parquet)
    tfm.CATEGORICAL_FEATURES, tfm.NUMERIC_FEATURES, tfm.FEATURE_COLS = _resolve_feature_cols(df)
    X, _ = encode_features(df)
    y = df["outcome"].values.astype(int)
    courts_all = df["_court_id"].astype(str).values
    X_tr, X_te, y_tr, y_te, c_tr, c_te = train_test_split(
        X, y, courts_all, test_size=0.2, random_state=42, stratify=y
    )
    model = load_model(run_id)
    p = predict_proba(model, X_te, c_te)
    from sklearn.metrics import brier_score_loss
    brier = brier_score_loss(y_te, p)
    ece, rows = reliability(y_te, p)
    print(f"\n=== {label} (run {run_id[:12]}) ===")
    print(f"  test n={len(y_te)}  Brier={brier:.4f}  ECE={ece:.4f}  base_rate={y_te.mean():.3f}  mean_pred={p.mean():.3f}")
    print(f"  {'bin':>12} {'n':>5} {'conf':>7} {'acc':>7} {'gap':>7}  signed")
    for lo, hi, cnt, conf, acc, gap in rows:
        if cnt == 0:
            continue
        signed = conf - acc
        flag = "  OVER" if signed > 0.02 else ("  under" if signed < -0.02 else "")
        print(f"  [{lo:.1f},{hi:.1f}) {cnt:>5} {conf:>7.3f} {acc:>7.3f} {gap:>7.3f}  {signed:+.3f}{flag}")
    return p, y_te


if __name__ == "__main__":
    evaluate("v14 MiniLM", "data/real_corpus_v14.parquet", "8ba01003c252491eb5edb4c0138e11df")
    evaluate("v17 legal-BERT", "data/real_corpus_v17.parquet", "6ef82aa2764641e39fc083c851f7edba")
