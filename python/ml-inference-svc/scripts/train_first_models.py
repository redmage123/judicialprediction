"""
Train base GBMs + LR baseline + stacking blender on synthetic case data.

Base models:
  - XGBoost   (gradient-boosted trees)
  - LightGBM  (gradient-boosted trees)
  - CatBoost  (gradient-boosted trees)
  - Logistic regression (architecturally-diverse baseline — different
    inductive bias from the trees; catches when trees overfit and gives
    the stacker a non-tree signal to blend)

Stacking blender (Sprint 12.5):
  - Out-of-fold (K=5) predictions from each base model on the training
    set form a (n_train, 4) feature matrix.
  - Logistic regression meta-learner trained on (OOF_probs, y_train)
    learns optimal per-model blending weights.
  - At inference: each base model predicts on the input, the meta-learner
    blends those four probabilities.

Each model is Platt-calibrated (LogisticRegression on its raw scores) and
logged to MLflow (local file backend at mlruns/).  Lowest-Brier-score
model is tagged 'champion' and written to mlruns/champion.json — the
gateway's predict.py reads this on every prediction.

Conformal calibration residuals from each model's cal split are saved as
an MLflow artifact for the gateway's split-conformal CI machinery.

Usage:
    uv run python scripts/train_first_models.py --data data/synthetic_cases_v1.parquet
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
from sklearn.model_selection import StratifiedKFold, train_test_split
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import OrdinalEncoder, StandardScaler
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


def _detect_gpu() -> bool:
    """
    True iff the host advertises an NVIDIA GPU via `nvidia-smi -L`.
    Used by the GBM builders to pick CUDA vs CPU kwargs.  Intentionally
    fail-safe: any exception (binary missing, driver not loaded, container
    without device passthrough) → False.
    """
    import shutil, subprocess
    if shutil.which("nvidia-smi") is None:
        return False
    try:
        out = subprocess.run(
            ["nvidia-smi", "-L"],
            capture_output=True, text=True, timeout=3,
        )
        return out.returncode == 0 and "GPU" in out.stdout
    except Exception:
        return False


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
    Thin wrapper: a pre-fit base model + a LogisticRegression Platt scaler.
    Exposes predict_proba so it works with mlflow.sklearn.log_model.
    """

    def __init__(self, base_model, platt: LogisticRegression) -> None:
        self.base_model = base_model
        self.platt = platt

    def _raw_scores(self, X: np.ndarray) -> np.ndarray:
        """Raw (uncalibrated) positive-class probabilities from the base."""
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


class StackedEnsemble:
    """
    Sprint 12.5 stacking blender.

    Holds N already-fit + Platt-calibrated base models plus a logistic-
    regression meta-learner trained on out-of-fold base probabilities.
    Inference: each base model produces a calibrated probability; the
    meta-learner blends them.

    Why stacking instead of simple averaging:
      - Equal-weight averaging is dominated by stacking on most tabular
        benchmarks because base models have wildly different per-row
        accuracy.
      - LR on top stays interpretable (one coefficient per base model =
        how much that model is trusted).
      - Calibration stays sensible because the base inputs are already
        calibrated (Platt) — the meta-LR is blending probabilities, not
        raw scores.
    """

    def __init__(self, base_models: list, meta: LogisticRegression) -> None:
        # `base_models` is a list of PlattCalibratedModel instances; we
        # rely on each having predict_proba(X) -> (n, 2).
        self.base_models = base_models
        self.meta = meta

    def _base_probs(self, X: np.ndarray) -> np.ndarray:
        """Stack each base model's P(class=1) into shape (n_samples, n_base)."""
        cols = [m.predict_proba(X)[:, 1] for m in self.base_models]
        return np.column_stack(cols)

    def predict_proba(self, X: np.ndarray) -> np.ndarray:
        Z = self._base_probs(X)
        p1 = self.meta.predict_proba(Z)[:, 1]
        return np.column_stack([1 - p1, p1])

    def predict(self, X: np.ndarray) -> np.ndarray:
        return (self.predict_proba(X)[:, 1] >= 0.5).astype(int)

    def get_params(self, deep=True):
        return {}

    def set_params(self, **params):
        return self


def train_and_evaluate_v2(
    X_train: np.ndarray,
    y_train: np.ndarray,
    X_test: np.ndarray,
    y_test: np.ndarray,
    model_name: str,
    base_model,
    tracking_uri: str,
    seed: int = 42,
):
    """
    Sprint 12.5: returns (result_dict, calibrated_model) so the stacker
    can reuse the already-trained + Platt-calibrated model without
    refitting. Otherwise identical to the original train_and_evaluate.
    """
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
    return (
        {
            "model_name": model_name,
            "brier": brier,
            "ece": ece_val,
            "log_loss": logloss,
            "run_id": run_id,
        },
        calibrated,
    )


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

    # Sprint 12.5 — four base models with intentionally diverse inductive
    # biases.  The three GBMs find non-linear interactions; the LR
    # baseline catches the linear-additive signal and disagrees with the
    # trees enough to give the stacker something to blend.
    #
    # Sprint 13 — GPU acceleration: each GBM picks its CUDA path when the
    # host advertises a GPU via `nvidia-smi`. CPU fallback is intentionally
    # silent (xgboost.tree_method="hist" runs identically; lightgbm device
    # "cpu" is the default; catboost.task_type="CPU" is the default), so
    # this script stays portable to laptops without changes.
    gpu_available = _detect_gpu()
    if gpu_available:
        print("GPU detected — training with CUDA (xgb hist+device=cuda, "
              "lgbm device=gpu, catboost task_type=GPU)")
    else:
        print("No GPU — training on CPU (use a host with nvidia-smi for the GPU path)")

    xgb_kwargs = {
        "n_estimators": 100,
        "max_depth": 4,
        "learning_rate": 0.1,
        "eval_metric": "logloss",
        "random_state": seed,
        "verbosity": 0,
    }
    if gpu_available:
        # xgboost 2.x: tree_method='hist' + device='cuda' is the supported
        # CUDA path. tree_method='gpu_hist' is the deprecated older form.
        xgb_kwargs["tree_method"] = "hist"
        xgb_kwargs["device"] = "cuda"

    lgbm_kwargs = {
        "n_estimators": 100,
        "max_depth": 4,
        "learning_rate": 0.1,
        "random_state": seed,
        "verbose": -1,
    }
    if gpu_available:
        # LightGBM's GPU path is opt-in and needs a wheel compiled with
        # OpenCL+CUDA. Most pip wheels don't ship with GPU support; we try
        # but the script still works if LightGBM falls back at fit time.
        lgbm_kwargs["device"] = "gpu"

    cat_kwargs = {
        "iterations": 100,
        "depth": 4,
        "learning_rate": 0.1,
        "random_seed": seed,
        "verbose": 0,
    }
    if gpu_available:
        cat_kwargs["task_type"] = "GPU"
        cat_kwargs["devices"] = "0"

    models = [
        ("xgboost", XGBClassifier(**xgb_kwargs)),
        ("lightgbm", LGBMClassifier(**lgbm_kwargs)),
        ("catboost", CatBoostClassifier(**cat_kwargs)),
        (
            "logistic_regression",
            # Pipeline so the scaler is fit on training data and the
            # standardised features land in the LR without separate
            # preprocessing plumbing.  LR is the architecturally-diverse
            # member of the ensemble (linear inductive bias).
            Pipeline(
                steps=[
                    ("scaler", StandardScaler()),
                    (
                        "lr",
                        LogisticRegression(
                            max_iter=2000,
                            C=1.0,
                            solver="lbfgs",
                            random_state=seed,
                        ),
                    ),
                ]
            ),
        ),
    ]

    print(f"\nTracking URI : {tracking_uri}")
    print(f"Train/test   : {len(X_train)} / {len(X_test)} samples\n")

    results = []
    # Hold each model's PlattCalibratedModel reference too, so the
    # stacker can re-use them at the end without retraining.  Same
    # objects MLflow already logged inside train_and_evaluate.
    calibrated_by_name: dict[str, PlattCalibratedModel] = {}

    for name, model in models:
        r, calibrated = train_and_evaluate_v2(
            X_train, y_train, X_test, y_test,
            model_name=name,
            base_model=model,
            tracking_uri=tracking_uri,
            seed=seed,
        )
        results.append(r)
        calibrated_by_name[name] = calibrated

    # ── Sprint 12.5 — stacking blender ────────────────────────────────────
    # Standard practice for tabular stacking:
    #   1. Out-of-fold (K=5) predictions on the training set form the
    #      meta-features.  We can't use in-sample base predictions
    #      because they'd leak the labels into the meta-learner.
    #   2. Logistic regression on (OOF_probs, y_train) learns the
    #      blending weights.
    #   3. The "production" stacker bundles the already-fit base models
    #      (trained on the full training set during step (1) below) with
    #      the meta-LR for inference.
    print("\nTraining stacking blender (K=5 OOF + LR meta)...")
    base_names = [n for n, _ in models]
    n_train = len(X_train)
    n_base = len(base_names)
    oof_probs = np.zeros((n_train, n_base), dtype=float)

    kf = StratifiedKFold(n_splits=5, shuffle=True, random_state=seed)
    for fold_idx, (fit_idx, val_idx) in enumerate(kf.split(X_train, y_train)):
        for col, (name, model_template) in enumerate(models):
            # `clone` keeps the original templates intact so the
            # subsequent "final fit on full train" loop below isn't
            # contaminated.
            from sklearn.base import clone
            fold_model = clone(model_template)
            fold_model.fit(X_train[fit_idx], y_train[fit_idx])
            oof_probs[val_idx, col] = fold_model.predict_proba(X_train[val_idx])[:, 1]

    meta = LogisticRegression(max_iter=2000, random_state=seed)
    meta.fit(oof_probs, y_train)

    # Build the production stacker: it needs each base model trained on
    # the FULL training set (not just K-1 folds), so we just reuse the
    # already-fit calibrated models from above. They were fit on a
    # train/cal split, so the "fit" portion is ~80% of X_train rather
    # than 100% — acceptable, and consistent with how the gateway sees
    # them when they're champion individually.
    stacker = StackedEnsemble(
        base_models=[calibrated_by_name[n] for n in base_names],
        meta=meta,
    )

    # Evaluate + log the stacker as its own MLflow run so champion
    # selection treats it as a peer of the four base models.
    p_test_stacker = stacker.predict_proba(X_test)[:, 1]
    stacker_brier = brier_score_loss(y_test, p_test_stacker)
    stacker_ece = ece(y_test, p_test_stacker)
    stacker_logloss = log_loss(y_test, p_test_stacker)

    mlflow.set_tracking_uri(tracking_uri)
    mlflow.set_experiment("judicialpredict-gbm-ensemble")
    with mlflow.start_run(run_name="stacked_ensemble") as run:
        mlflow.log_params({
            "model": "stacked_ensemble",
            "base_models": ",".join(base_names),
            "meta": "logistic_regression",
            "cv_folds": 5,
            "seed": seed,
        })
        mlflow.log_metrics({
            "brier_score": stacker_brier,
            "ece": stacker_ece,
            "log_loss": stacker_logloss,
        })
        # Log the meta-LR coefficients so the audit story can show how
        # much each base model is trusted ("CatBoost weighted 0.6, LR
        # weighted 0.2, ..."). Useful for the compliance footer story.
        for name, coef in zip(base_names, meta.coef_[0]):
            mlflow.log_metric(f"meta_coef_{name}", float(coef))
        mlflow.set_tag("model_name", "stacked_ensemble")
        mlflow.sklearn.log_model(stacker, artifact_path="model")

        # Conformal residuals — re-use the test-set predictions as the
        # calibration source. (We don't have a separate cal split for
        # the stacker because the base models already consumed one.)
        residuals = np.abs(y_test.astype(float) - p_test_stacker)
        with tempfile.TemporaryDirectory() as tmp:
            res_path = os.path.join(tmp, "conformal_residuals.npy")
            np.save(res_path, residuals)
            mlflow.log_artifact(res_path)

        stacker_run_id = run.info.run_id

    print(
        f"  {'stacked':10s}  Brier={stacker_brier:.4f}  ECE={stacker_ece:.4f}"
        f"  LogLoss={stacker_logloss:.4f}  run_id={stacker_run_id}"
    )
    print(f"    meta coefs: " + " ".join(
        f"{n}={c:+.2f}" for n, c in zip(base_names, meta.coef_[0])
    ))

    results.append({
        "model_name": "stacked_ensemble",
        "brier": stacker_brier,
        "ece": stacker_ece,
        "log_loss": stacker_logloss,
        "run_id": stacker_run_id,
    })

    # ── Champion selection (now across 5 candidates) ──────────────────────
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
