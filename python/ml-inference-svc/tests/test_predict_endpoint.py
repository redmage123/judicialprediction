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
    The promoted champion's recorded ECE must be < 0.10.

    Pre-S20.6 this re-derived ECE from freshly generated synthetic cases via
    the now-removed `_encode_features` path. The current champion is the
    text-conditional real-data model (S20.6+): it requires an `opinion_text`
    embedding and a feature contract, so synthetic cases without opinion text
    cannot exercise it. Instead we assert the calibration metric recorded at
    promotion time (champion.json `ece`), which is exactly the number the
    promotion gate enforces. Skipped if no champion is present.
    """
    from ml_inference_svc.predict import _champion_meta

    try:
        meta = _champion_meta()
    except FileNotFoundError:
        pytest.skip("Champion model not found — run train_first_models.py first")

    ece_val = meta.get("ece")
    assert ece_val is not None, "champion.json must record an `ece` calibration metric"
    assert ece_val < 0.10, f"Calibration ECE {ece_val:.4f} >= 0.10 threshold"
