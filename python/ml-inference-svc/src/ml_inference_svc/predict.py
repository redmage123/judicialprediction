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


def _resolve_run_artifact_path(
    project_root: str, run_id: str, artifact_name: str
) -> str | None:
    """
    Return the absolute filesystem path to a per-run artifact, or None.

    Run-scoped artifacts live at
    ``mlruns/<exp>/<run_id>/artifacts/<artifact_name>``.  Reading directly
    side-steps the MLflowClient.download_artifacts plumbing that breaks on
    MLflow 3's revised layout.
    """
    mlruns_root = os.path.join(project_root, "mlruns")
    if not os.path.isdir(mlruns_root):
        return None
    for exp in os.listdir(mlruns_root):
        candidate = os.path.join(
            mlruns_root, exp, run_id, "artifacts", artifact_name
        )
        if os.path.isfile(candidate):
            return candidate
    return None


def _resolve_logged_model_path(project_root: str, run_id: str) -> str | None:
    """
    Return the absolute filesystem path to the logged model artifacts dir, or
    None if no model logged from this run can be located.

    MLflow 3 separates logged models from runs: a `log_model(name="model")`
    call writes the model to
    ``mlruns/<exp>/models/m-<id>/artifacts/`` (with ``MLmodel`` and
    ``model.pkl`` inside) and records the linkage in
    ``mlruns/<exp>/<run>/outputs/m-<id>``.

    Loading via the documented ``models:/<id>`` URI hits a MLflow 3.12 bug
    where the inner LocalArtifactRepository receives an empty artifact_path
    and throws ``No such artifact: ''``.  Loading via the absolute artifacts
    directory on disk works reliably and side-steps the URI plumbing.
    """
    mlruns_root = os.path.join(project_root, "mlruns")
    if not os.path.isdir(mlruns_root):
        return None
    # Experiment dirs are numeric; the only non-numeric entry is `.trash`.
    for exp in os.listdir(mlruns_root):
        outputs_dir = os.path.join(mlruns_root, exp, run_id, "outputs")
        if not os.path.isdir(outputs_dir):
            continue
        for name in os.listdir(outputs_dir):
            if not name.startswith("m-"):
                continue
            artifacts = os.path.join(mlruns_root, exp, "models", name, "artifacts")
            if os.path.isfile(os.path.join(artifacts, "MLmodel")):
                return artifacts
    return None


@lru_cache(maxsize=1)
def _load_champion():
    """Load and cache the champion sklearn model + conformal predictor."""
    meta = _champion_meta()
    run_id = meta["run_id"]

    here = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(os.path.dirname(here))
    tracking_uri = "file://" + os.path.join(project_root, "mlruns")

    mlflow.set_tracking_uri(tracking_uri)
    model_path = _resolve_logged_model_path(project_root, run_id)
    model_uri = model_path if model_path else f"runs:/{run_id}/model"
    model = mlflow.sklearn.load_model(model_uri)

    # Load conformal residuals artifact. MLflowClient.download_artifacts hits
    # the same MLflow 3.12 path-resolution bug as load_model("models:/..."),
    # so resolve the absolute path on disk and load with numpy directly.
    residuals_path = _resolve_run_artifact_path(project_root, run_id, "conformal_residuals.npy")
    if residuals_path is None:
        raise FileNotFoundError(
            f"Conformal residuals artifact not found for run {run_id}. "
            "Re-run train_first_models.py."
        )
    residuals = np.load(residuals_path)
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
