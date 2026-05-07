"""
Train XGBoost + LightGBM + CatBoost on synthetic case data.
Calibrates predictions via Platt scaling (LogisticRegression on model scores).
Logs each run to MLflow (local file backend at mlruns/).
Tags the lowest-Brier-score model as 'champion'.
Saves conformal calibration residuals as an MLflow artifact.

Usage:
    uv run python scripts/train_first_models.py --data data/synthetic_cases_v0.parquet
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
from catboost import CatBoostClassifier
from lightgbm import LGBMClassifier
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import brier_score_loss, log_loss
from sklearn.model_selection import train_test_split
from sklearn.preprocessing import OrdinalEncoder
from xgboost import XGBClassifier

CATEGORICAL_FEATURES = ["case_type", "jurisdiction"]
NUMERIC_FEATURES = [
    "judge_severity",
    "attorney_win_rate",
    "ideology_distance",
    "materiality_score",
    "procedural_motion_count",
]
FEATURE_COLS = NUMERIC_FEATURES + CATEGORICAL_FEATURES
TARGET_COL = "outcome"


def ece(y_true: np.ndarray, y_prob: np.ndarray, n_bins: int = 10) -> float:
    """Expected Calibration Error — equal-width bins."""
    bins = np.linspace(0.0, 1.0, n_bins + 1)
    ece_val = 0.0
    for lo, hi in zip(bins[:-1], bins[1:]):
        mask = (y_prob >= lo) & (y_prob < hi)
        if mask.sum() == 0:
            continue
        bin_conf = float(y_prob[mask].mean())
        bin_acc = float(y_true[mask].mean())
        ece_val += (mask.sum() / len(y_true)) * abs(bin_conf - bin_acc)
    return float(ece_val)


def encode_features(df: pd.DataFrame, encoder: OrdinalEncoder | None = None):
    """Ordinal-encode categoricals; returns (X_np, encoder)."""
    X = df[FEATURE_COLS].copy()
    if encoder is None:
        encoder = OrdinalEncoder(handle_unknown="use_encoded_value", unknown_value=-1)
        X[CATEGORICAL_FEATURES] = encoder.fit_transform(X[CATEGORICAL_FEATURES])
    else:
        X[CATEGORICAL_FEATURES] = encoder.transform(X[CATEGORICAL_FEATURES])
    return X.values.astype(float), encoder


class PlattCalibratedModel:
    """
    Thin wrapper: a pre-fit GBM + a LogisticRegression Platt scaler.
    Exposes predict_proba so it works with mlflow.sklearn.log_model.
    """

    def __init__(self, base_model, platt: LogisticRegression) -> None:
        self.base_model = base_model
        self.platt = platt

    def _raw_scores(self, X: np.ndarray) -> np.ndarray:
        """Raw (uncalibrated) positive-class probabilities from the GBM."""
        return self.base_model.predict_proba(X)[:, 1].reshape(-1, 1)

    def predict_proba(self, X: np.ndarray) -> np.ndarray:
        scores = self._raw_scores(X)
        p1 = self.platt.predict_proba(scores)[:, 1]
        return np.column_stack([1 - p1, p1])

    def predict(self, X: np.ndarray) -> np.ndarray:
        return (self.predict_proba(X)[:, 1] >= 0.5).astype(int)

    # sklearn compatibility
    def get_params(self, deep=True):
        return {}

    def set_params(self, **params):
        return self


def train_and_evaluate(
    X_train: np.ndarray,
    y_train: np.ndarray,
    X_test: np.ndarray,
    y_test: np.ndarray,
    model_name: str,
    base_model,
    tracking_uri: str,
    seed: int = 42,
) -> dict:
    mlflow.set_tracking_uri(tracking_uri)
    mlflow.set_experiment("judicialpredict-gbm-ensemble")

    with mlflow.start_run(run_name=model_name) as run:
        # Reserve a calibration slice for Platt scaling.
        cal_size = max(50, int(0.2 * len(X_train)))
        X_fit, X_cal, y_fit, y_cal = train_test_split(
            X_train, y_train, test_size=cal_size, random_state=seed, stratify=y_train
        )

        base_model.fit(X_fit, y_fit)

        # Platt scaling: fit a logistic regression on raw scores vs labels.
        raw_cal = base_model.predict_proba(X_cal)[:, 1].reshape(-1, 1)
        platt = LogisticRegression(max_iter=1000, random_state=seed)
        platt.fit(raw_cal, y_cal)

        calibrated = PlattCalibratedModel(base_model, platt)

        p_test = calibrated.predict_proba(X_test)[:, 1]
        brier = brier_score_loss(y_test, p_test)
        ece_val = ece(y_test, p_test)
        logloss = log_loss(y_test, p_test)

        mlflow.log_params({"model": model_name, "seed": seed, "cal_size": cal_size})
        mlflow.log_metrics({"brier_score": brier, "ece": ece_val, "log_loss": logloss})
        mlflow.set_tag("model_name", model_name)
        mlflow.sklearn.log_model(calibrated, artifact_path="model")

        # Conformal calibration residuals from the cal split.
        p_cal_pred = calibrated.predict_proba(X_cal)[:, 1]
        residuals = np.abs(y_cal.astype(float) - p_cal_pred)
        with tempfile.TemporaryDirectory() as tmp:
            res_path = os.path.join(tmp, "conformal_residuals.npy")
            np.save(res_path, residuals)
            mlflow.log_artifact(res_path)

        run_id = run.info.run_id

    print(
        f"  {model_name:10s}  Brier={brier:.4f}  ECE={ece_val:.4f}"
        f"  LogLoss={logloss:.4f}  run_id={run_id}"
    )
    return {
        "model_name": model_name,
        "brier": brier,
        "ece": ece_val,
        "log_loss": logloss,
        "run_id": run_id,
    }


def main(data_path: str, seed: int = 42) -> None:
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(script_dir)
    tracking_uri = "file://" + os.path.join(project_root, "mlruns")

    df = pd.read_parquet(data_path)
    X, _encoder = encode_features(df)
    y = df[TARGET_COL].values.astype(int)

    X_train, X_test, y_train, y_test = train_test_split(
        X, y, test_size=0.2, random_state=seed, stratify=y
    )

    models = [
        (
            "xgboost",
            XGBClassifier(
                n_estimators=100,
                max_depth=4,
                learning_rate=0.1,
                eval_metric="logloss",
                random_state=seed,
                verbosity=0,
            ),
        ),
        (
            "lightgbm",
            LGBMClassifier(
                n_estimators=100,
                max_depth=4,
                learning_rate=0.1,
                random_state=seed,
                verbose=-1,
            ),
        ),
        (
            "catboost",
            CatBoostClassifier(
                iterations=100,
                depth=4,
                learning_rate=0.1,
                random_seed=seed,
                verbose=0,
            ),
        ),
    ]

    print(f"\nTracking URI : {tracking_uri}")
    print(f"Train/test   : {len(X_train)} / {len(X_test)} samples\n")

    results = []
    for name, model in models:
        r = train_and_evaluate(
            X_train, y_train, X_test, y_test,
            model_name=name,
            base_model=model,
            tracking_uri=tracking_uri,
            seed=seed,
        )
        results.append(r)

    champion = min(results, key=lambda r: r["brier"])
    mlflow.set_tracking_uri(tracking_uri)
    client = mlflow.tracking.MlflowClient()
    client.set_tag(champion["run_id"], "champion", "true")

    meta_path = os.path.join(project_root, "mlruns", "champion.json")
    with open(meta_path, "w") as f:
        json.dump(champion, f, indent=2)

    print(f"\nChampion     : {champion['model_name']}  run_id={champion['run_id']}")
    print(f"Metadata     : {meta_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", required=True, help="Path to synthetic cases parquet")
    parser.add_argument("--seed", type=int, default=42)
    args = parser.parse_args()
    main(args.data, args.seed)
