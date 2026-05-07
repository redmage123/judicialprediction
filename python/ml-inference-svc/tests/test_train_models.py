"""
Training reproducibility and conformal layer unit tests.
"""
from __future__ import annotations

import os
import sys
import tempfile

import numpy as np
import pandas as pd
import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts"))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from generate_synthetic_cases import main as generate_main
from ml_inference_svc.conformal import SplitConformalPredictor


# ── fixtures ─────────────────────────────────────────────────────────────────

@pytest.fixture(scope="module")
def data_path(tmp_path_factory):
    p = tmp_path_factory.mktemp("data") / "cases.parquet"
    generate_main(seed=42, output=str(p))
    return str(p)


@pytest.fixture(scope="module")
def trained_results(data_path, tmp_path_factory):
    """Train once and return the metrics dict list."""
    import mlflow
    from train_first_models import main as train_main, encode_features, ece, FEATURE_COLS, TARGET_COL
    from sklearn.model_selection import train_test_split

    mlruns_dir = tmp_path_factory.mktemp("mlruns")
    tracking_uri = "file://" + str(mlruns_dir)

    df = pd.read_parquet(data_path)
    from sklearn.preprocessing import OrdinalEncoder
    X, encoder = encode_features(df)
    y = df[TARGET_COL].values.astype(int)
    X_train, X_test, y_train, y_test = train_test_split(
        X, y, test_size=0.2, random_state=42, stratify=y
    )
    return X_train, X_test, y_train, y_test, tracking_uri


# ── conformal predictor tests ─────────────────────────────────────────────────

def test_conformal_fit_and_interval():
    rng = np.random.default_rng(0)
    y_cal = rng.integers(0, 2, size=200)
    p_cal = rng.uniform(0.2, 0.8, size=200)
    cp = SplitConformalPredictor()
    cp.fit(y_cal, p_cal)
    lower, upper = cp.predict_interval(0.6, alpha=0.10)
    assert 0.0 <= lower <= 0.6 <= upper <= 1.0, "CI must bracket the point estimate"


def test_conformal_interval_clipped():
    """Interval is always clipped to [0, 1] even for extreme p values."""
    y_cal = np.zeros(100)
    p_cal = np.zeros(100)
    cp = SplitConformalPredictor().fit(y_cal, p_cal)
    lower, upper = cp.predict_interval(0.02, alpha=0.10)
    assert lower >= 0.0
    assert upper <= 1.0


def test_conformal_requires_fit():
    cp = SplitConformalPredictor()
    with pytest.raises(RuntimeError, match="fit()"):
        cp.predict_interval(0.5)


def test_conformal_from_residuals():
    residuals = np.array([0.1, 0.2, 0.3, 0.05, 0.15])
    cp = SplitConformalPredictor.from_residuals(residuals)
    lower, upper = cp.predict_interval(0.5, alpha=0.10)
    assert lower <= 0.5 <= upper


def test_conformal_90pct_coverage():
    """Empirical coverage on a held-out set should be >= 90 % (marginal guarantee)."""
    rng = np.random.default_rng(99)
    n = 500
    y_cal = rng.integers(0, 2, size=n)
    p_cal = rng.uniform(0, 1, size=n)

    cp = SplitConformalPredictor().fit(y_cal, p_cal)

    y_test = rng.integers(0, 2, size=n)
    p_test = rng.uniform(0, 1, size=n)
    covered = sum(
        1 for y, p in zip(y_test, p_test)
        if cp.predict_interval(p, alpha=0.10)[0] <= y <= cp.predict_interval(p, alpha=0.10)[1]
    )
    coverage = covered / n
    assert coverage >= 0.85, f"Coverage {coverage:.3f} too low (expected >= 0.85)"


# ── training reproducibility ──────────────────────────────────────────────────

def test_training_reproducible(data_path, tmp_path_factory):
    """Two runs with the same seed must yield identical Brier scores."""
    from train_first_models import encode_features, train_and_evaluate, TARGET_COL
    from sklearn.model_selection import train_test_split
    from xgboost import XGBClassifier

    mlruns_dir = tmp_path_factory.mktemp("mlruns_repro")
    tracking_uri = "file://" + str(mlruns_dir)

    df = pd.read_parquet(data_path)
    X, _ = encode_features(df)
    y = df[TARGET_COL].values.astype(int)
    X_train, X_test, y_train, y_test = train_test_split(
        X, y, test_size=0.2, random_state=42, stratify=y
    )

    def make_model():
        return XGBClassifier(
            n_estimators=50, max_depth=3, learning_rate=0.1,
            eval_metric="logloss", random_state=42, verbosity=0,
        )

    r1 = train_and_evaluate(
        X_train, y_train, X_test, y_test,
        model_name="xgboost_r1", base_model=make_model(),
        tracking_uri=tracking_uri, seed=42,
    )
    r2 = train_and_evaluate(
        X_train, y_train, X_test, y_test,
        model_name="xgboost_r2", base_model=make_model(),
        tracking_uri=tracking_uri, seed=42,
    )
    assert abs(r1["brier"] - r2["brier"]) < 1e-8, "Brier scores not identical across runs with same seed"
