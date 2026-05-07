"""
Inference pipeline for JudicialPredict ml-inference-svc.

predict_case_outcome(features) -> (p_win, ci_lower, ci_upper, model_version)

The champion model and conformal residuals are loaded lazily on first call from
the mlruns/champion.json pointer written by train_first_models.py.
"""
from __future__ import annotations

import json
import os
from functools import lru_cache

import mlflow
import numpy as np

from ml_inference_svc.conformal import SplitConformalPredictor

# Ordered feature list that the model expects (must match train_first_models.py).
FEATURE_ORDER = [
    "judge_severity",
    "attorney_win_rate",
    "ideology_distance",
    "materiality_score",
    "procedural_motion_count",
    "case_type",
    "jurisdiction",
]

# Tier-A/B allowlist — Tier-C party features are never accepted here.
ALLOWLIST_FEATURES: frozenset[str] = frozenset(FEATURE_ORDER)

# Categorical ordinal encoding maps (mirrors train_first_models.py OrdinalEncoder fit order).
_CASE_TYPE_MAP = {"civil": 0.0, "criminal": 1.0, "bankruptcy": 2.0}
_JURISDICTION_MAP = {"California": 0.0, "Federal": 1.0, "New_Jersey": 2.0}


def _encode_features(features: dict) -> np.ndarray:
    """Convert a feature dict to the numpy row vector the model expects."""
    row = [
        float(features["judge_severity"]),
        float(features["attorney_win_rate"]),
        float(features["ideology_distance"]),
        float(features["materiality_score"]),
        float(features["procedural_motion_count"]),
        _CASE_TYPE_MAP.get(str(features["case_type"]), -1.0),
        _JURISDICTION_MAP.get(str(features["jurisdiction"]), -1.0),
    ]
    return np.array(row, dtype=float).reshape(1, -1)


def _champion_meta() -> dict:
    """Read champion.json written by train_first_models.py."""
    here = os.path.dirname(os.path.abspath(__file__))
    # Traverse: src/ml_inference_svc -> src -> project root
    project_root = os.path.dirname(os.path.dirname(here))
    meta_path = os.path.join(project_root, "mlruns", "champion.json")
    if not os.path.exists(meta_path):
        raise FileNotFoundError(
            f"Champion metadata not found at {meta_path}. "
            "Run train_first_models.py first."
        )
    with open(meta_path) as f:
        return json.load(f)


@lru_cache(maxsize=1)
def _load_champion():
    """Load and cache the champion sklearn model + conformal predictor."""
    meta = _champion_meta()
    run_id = meta["run_id"]

    here = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(os.path.dirname(here))
    tracking_uri = "file://" + os.path.join(project_root, "mlruns")

    mlflow.set_tracking_uri(tracking_uri)
    model_uri = f"runs:/{run_id}/model"
    model = mlflow.sklearn.load_model(model_uri)

    # Load conformal residuals artifact.
    client = mlflow.tracking.MlflowClient()
    local_dir = client.download_artifacts(run_id, "conformal_residuals.npy")
    residuals = np.load(local_dir)
    conformal = SplitConformalPredictor.from_residuals(residuals)

    return model, conformal, meta


def predict_case_outcome(
    features: dict,
    alpha: float = 0.10,
) -> tuple[float, float, float, str]:
    """
    Return (p_win, ci_lower, ci_upper, model_version) for a feature dict.

    Args:
        features: Dict mapping feature names to values. Must contain exactly
                  the keys in ALLOWLIST_FEATURES; Tier-C keys are rejected upstream.
        alpha: Conformal error level (0.10 => 90 % CI).

    Returns:
        p_win      — calibrated win probability in [0, 1].
        ci_lower   — conformal lower bound in [0, 1].
        ci_upper   — conformal upper bound in [0, 1].
        model_version — MLflow run_id of the champion model.
    """
    model, conformal, meta = _load_champion()
    X = _encode_features(features)
    p_win = float(model.predict_proba(X)[0, 1])
    ci_lower, ci_upper = conformal.predict_interval(p_win, alpha=alpha)
    return p_win, ci_lower, ci_upper, meta["run_id"]
