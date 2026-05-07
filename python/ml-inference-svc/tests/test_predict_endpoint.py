"""
FastAPI /predict endpoint tests.

Tests use a monkeypatched predict_case_outcome so they do not require a
pre-trained champion model on disk. A separate integration-style fixture
optionally tests calibration error when a champion model is available.
"""
from __future__ import annotations

import os
import sys
import tempfile

import numpy as np
import pytest
from fastapi.testclient import TestClient

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

VALID_PAYLOAD = {
    "judge_severity": 0.6,
    "attorney_win_rate": 0.7,
    "case_type": "civil",
    "jurisdiction": "Federal",
    "ideology_distance": 0.3,
    "materiality_score": 0.8,
    "procedural_motion_count": 5,
}

MOCK_RUN_ID = "test-run-abc123"


@pytest.fixture()
def client(monkeypatch):
    """Return a TestClient with predict_case_outcome mocked."""
    import ml_inference_svc.main as main_module

    def mock_predict(features, alpha=0.10):
        return 0.65, 0.45, 0.85, MOCK_RUN_ID

    monkeypatch.setattr(main_module, "predict_case_outcome", mock_predict)
    return TestClient(main_module.app)


# ── happy-path tests ──────────────────────────────────────────────────────────

def test_predict_returns_200(client):
    resp = client.post("/predict", json=VALID_PAYLOAD)
    assert resp.status_code == 200


def test_p_win_in_unit_interval(client):
    resp = client.post("/predict", json=VALID_PAYLOAD)
    data = resp.json()
    assert 0.0 <= data["p_win"] <= 1.0


def test_ci_brackets_p_win(client):
    resp = client.post("/predict", json=VALID_PAYLOAD)
    data = resp.json()
    assert data["ci_lower"] <= data["p_win"] <= data["ci_upper"]


def test_coverage_is_90pct(client):
    resp = client.post("/predict", json=VALID_PAYLOAD)
    assert resp.json()["coverage"] == pytest.approx(0.90)


def test_model_version_returned(client):
    resp = client.post("/predict", json=VALID_PAYLOAD)
    assert resp.json()["model_version"] == MOCK_RUN_ID


def test_predicted_at_unix_present(client):
    resp = client.post("/predict", json=VALID_PAYLOAD)
    assert isinstance(resp.json()["predicted_at_unix"], int)


# ── Tier-C rejection tests ─────────────────────────────────────────────────────

@pytest.mark.parametrize("tier_c_field", [
    "party_race",
    "party_gender",
    "party_age",
    "party_ethnicity",
    "tier_c_field",
    "immigration_status",
    "disability_status",
])
def test_tier_c_field_rejected(client, tier_c_field):
    """Any Tier-C field must be rejected with HTTP 400."""
    payload = {**VALID_PAYLOAD, tier_c_field: "some_value"}
    resp = client.post("/predict", json=payload)
    assert resp.status_code == 400
    assert "Forbidden" in resp.json()["detail"] or "allowlist" in resp.json()["detail"].lower()


def test_missing_required_field_rejected(client):
    payload = {k: v for k, v in VALID_PAYLOAD.items() if k != "judge_severity"}
    resp = client.post("/predict", json=payload)
    assert resp.status_code == 400


# ── calibration test (requires trained model) ─────────────────────────────────

def test_calibration_error_below_threshold():
    """
    ECE on held-out data must be < 0.10.
    Skipped if no champion.json is present (pre-training environment).
    """
    import importlib
    try:
        from ml_inference_svc.predict import _champion_meta, _load_champion, _encode_features, ALLOWLIST_FEATURES
        _champion_meta()  # will FileNotFoundError if not trained
    except FileNotFoundError:
        pytest.skip("Champion model not found — run train_first_models.py first")

    sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts"))
    from generate_synthetic_cases import main as gen_main

    with tempfile.TemporaryDirectory() as tmp:
        data_path = os.path.join(tmp, "cases.parquet")
        gen_main(seed=99, output=data_path)

        import pandas as pd
        from sklearn.metrics import brier_score_loss
        from train_first_models import encode_features, TARGET_COL

        df = pd.read_parquet(data_path)
        X, _ = encode_features(df)
        y = df[TARGET_COL].values.astype(int)

        model, _, _ = _load_champion()
        p = model.predict_proba(X)[:, 1]

        # ECE
        n_bins = 10
        bins = np.linspace(0, 1, n_bins + 1)
        ece_val = 0.0
        for lo, hi in zip(bins[:-1], bins[1:]):
            mask = (p >= lo) & (p < hi)
            if mask.sum() == 0:
                continue
            ece_val += (mask.sum() / len(y)) * abs(p[mask].mean() - y[mask].mean())

        assert ece_val < 0.10, f"Calibration ECE {ece_val:.4f} >= 0.10 threshold"
