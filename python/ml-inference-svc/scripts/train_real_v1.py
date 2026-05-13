"""
Train JudicialPredict champion model v1 on real CourtListener-derived data.

S5.1 / S6.1 — real-corpus training under the explicit v1 sample-size caveat
the Sprint-5 risk plan pre-authorised:

    "if at 5 days in we're below 500 rows, train on what we have + flag in
    MODEL_CARD as v1, retrained when more data accumulates"

Current corpus (2026-05-13): 99 case_documents, 10 with hard binary labels
(8 respondent / 2 petitioner). This script does NOT pretend that's enough
data for a 100-tree GBM ensemble — it picks a small-N-appropriate model
(LogisticRegression) and reports leave-one-out CV metrics instead of a
holdout split (a holdout split of 2 rows is uninformative).

It reuses the inference plumbing from train_first_models.py:
  * Logs to the same MLflow tracking store at `mlruns/`.
  * Writes `mlruns/champion.json` with the run_id so predict.py picks it up.
  * Saves `conformal_residuals.npy` (LOOCV residuals) so the conformal
    predictor in ml_inference_svc.conformal still works.
  * Wraps the model so `predict_proba` is callable from predict.py.

What this script does NOT do:
  * Hold the model's hand by hyperparameter-searching on n=10 rows.
  * Pretend ECE on n=10 is statistically meaningful — it's reported for
    bookkeeping, not as a quality gate.

When the corpus grows beyond ~200 labelled rows, switch back to
train_first_models.py with the parquet this script doesn't consume.
"""
from __future__ import annotations

import argparse
import json
import os
import tempfile

import mlflow
import mlflow.sklearn
import numpy as np
import pandas as pd
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import brier_score_loss, log_loss
from sklearn.model_selection import LeaveOneOut
from sklearn.preprocessing import OrdinalEncoder

FEATURE_COLS = [
    "judge_severity",
    "attorney_win_rate",
    "ideology_distance",
    "materiality_score",
    "procedural_motion_count",
    "case_type",
    "jurisdiction",
]
CATEGORICAL = ["case_type", "jurisdiction"]
TARGET = "outcome"
MODEL_NAME = "logreg-v1-real"


def ece(y_true: np.ndarray, y_prob: np.ndarray, n_bins: int = 5) -> float:
    """Expected Calibration Error.  Bins=5 (not 10) given the small N."""
    bins = np.linspace(0.0, 1.0, n_bins + 1)
    total = float(len(y_true))
    val = 0.0
    for lo, hi in zip(bins[:-1], bins[1:]):
        mask = (y_prob >= lo) & (y_prob < hi)
        if mask.sum() == 0:
            continue
        bin_conf = float(y_prob[mask].mean())
        bin_acc = float(y_true[mask].mean())
        val += (mask.sum() / total) * abs(bin_conf - bin_acc)
    return val


def encode(df: pd.DataFrame, encoder: OrdinalEncoder | None = None):
    X = df[FEATURE_COLS].copy()
    if encoder is None:
        encoder = OrdinalEncoder(handle_unknown="use_encoded_value", unknown_value=-1)
        X[CATEGORICAL] = encoder.fit_transform(X[CATEGORICAL])
    else:
        X[CATEGORICAL] = encoder.transform(X[CATEGORICAL])
    return X.values.astype(float), encoder


class LogRegV1:
    """Trainer-shaped wrapper so mlflow.sklearn can log/load it the same
    way it logs the PlattCalibratedModel.  LogReg is already probabilistic
    so we don't need Platt scaling here."""

    def __init__(self, lr: LogisticRegression) -> None:
        self.lr = lr

    def predict_proba(self, X: np.ndarray) -> np.ndarray:
        return self.lr.predict_proba(X)

    def predict(self, X: np.ndarray) -> np.ndarray:
        return (self.predict_proba(X)[:, 1] >= 0.5).astype(int)

    def get_params(self, deep=True):
        return {}

    def set_params(self, **_params):
        return self


def loocv_predictions(X: np.ndarray, y: np.ndarray, seed: int) -> np.ndarray:
    """Leave-one-out predictions — each row predicted by a model fit on the
    other (n-1) rows.  With n in the low double digits the cost is trivial.

    Falls back to predicting the global base rate when fold has only one
    class present (e.g. all-respondent folds when the single petitioner
    is held out)."""
    n = len(X)
    out = np.empty(n, dtype=float)
    loo = LeaveOneOut()
    base_rate = float(y.mean())
    for train_idx, test_idx in loo.split(X):
        Xt, yt = X[train_idx], y[train_idx]
        if len(np.unique(yt)) < 2:
            # Degenerate fold — predict global base rate.
            out[test_idx[0]] = base_rate
            continue
        m = LogisticRegression(max_iter=1000, random_state=seed, class_weight="balanced")
        m.fit(Xt, yt)
        out[test_idx[0]] = float(m.predict_proba(X[test_idx])[0, 1])
    return out


def main(data_path: str, seed: int = 42, mlruns_dir: str | None = None) -> None:
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(script_dir)
    # Allow overriding the mlruns directory so trainers running inside the
    # ml-inference container (where /app/mlruns is mounted RO) can land
    # artefacts somewhere writable before they're docker-cp'd back to the
    # host mount.
    mlruns_dir = mlruns_dir or os.environ.get(
        "JP_MLRUNS_DIR", os.path.join(project_root, "mlruns")
    )
    tracking_uri = "file://" + mlruns_dir

    df = pd.read_parquet(data_path)
    if len(df) == 0:
        raise SystemExit("real corpus is empty")
    X, encoder = encode(df)
    y = df[TARGET].values.astype(int)
    n = len(X)
    n_pos = int(y.sum())
    base_rate = float(y.mean())

    print(f"\nReal-corpus v1 training")
    print(f"========================")
    print(f"Rows           : {n}")
    print(f"Petitioner wins: {n_pos}  (base rate {base_rate:.3f})")
    print(f"Tracking URI   : {tracking_uri}")

    mlflow.set_tracking_uri(tracking_uri)
    mlflow.set_experiment("judicialpredict-real-v1")

    with mlflow.start_run(run_name=MODEL_NAME) as run:
        # LOOCV first so we can log honest metrics.
        p_loocv = loocv_predictions(X, y, seed=seed)
        brier = brier_score_loss(y, p_loocv)
        # log_loss needs strictly-in-(0,1) probs; clip just in case.
        p_loocv_clip = np.clip(p_loocv, 1e-6, 1 - 1e-6)
        ll = log_loss(y, p_loocv_clip)
        ece_val = ece(y, p_loocv)

        # Now fit the final champion on all rows.  class_weight=balanced
        # so 2/10 petitioner rows still pull the boundary.
        lr = LogisticRegression(max_iter=1000, random_state=seed, class_weight="balanced")
        lr.fit(X, y)
        model = LogRegV1(lr)

        mlflow.log_params({
            "model": MODEL_NAME,
            "seed": seed,
            "n_rows": n,
            "n_pos": n_pos,
            "base_rate": base_rate,
            "evaluation": "leave-one-out CV",
        })
        mlflow.log_metrics({
            "brier_score": brier,
            "ece": ece_val,
            "log_loss": ll,
            "n_rows": float(n),
        })
        mlflow.set_tag("model_name", MODEL_NAME)
        mlflow.set_tag("real_corpus", "true")
        mlflow.set_tag("champion", "true")
        mlflow.sklearn.log_model(model, artifact_path="model")

        # Conformal residuals from the LOOCV predictions.  These feed the
        # SplitConformalPredictor in ml_inference_svc/conformal.py the same
        # way the GBM trainer's Platt-cal residuals did in S0.
        residuals = np.abs(y.astype(float) - p_loocv)
        with tempfile.TemporaryDirectory() as tmp:
            res_path = os.path.join(tmp, "conformal_residuals.npy")
            np.save(res_path, residuals)
            mlflow.log_artifact(res_path)

        run_id = run.info.run_id

    print(
        f"\n  {MODEL_NAME:18s}  Brier={brier:.4f}  ECE={ece_val:.4f}"
        f"  LogLoss={ll:.4f}  run_id={run_id}"
    )

    # Update champion pointer.
    meta = {
        "model_name": MODEL_NAME,
        "brier": brier,
        "ece": ece_val,
        "log_loss": ll,
        "run_id": run_id,
        "n_rows": n,
        "evaluation": "leave-one-out CV",
    }
    meta_path = os.path.join(mlruns_dir, "champion.json")
    os.makedirs(os.path.dirname(meta_path), exist_ok=True)
    with open(meta_path, "w") as f:
        json.dump(meta, f, indent=2)
    print(f"\nChampion JSON  : {meta_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", required=True)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument(
        "--mlruns-dir",
        default=None,
        help="Override the mlruns/ directory (also $JP_MLRUNS_DIR).",
    )
    args = parser.parse_args()
    main(args.data, args.seed, args.mlruns_dir)
