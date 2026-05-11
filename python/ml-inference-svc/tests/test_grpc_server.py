"""
test_grpc_server.py — in-process gRPC server tests for S5.3 / JP-70.

Spins up the gRPC server on an OS-assigned port (port 0), exercises the
InferenceService.PredictCaseOutcome RPC, and shuts down cleanly.

Three cases:
  a) Happy path:  valid Tier-A/B features → non-zero p_win + recommendation fields
  b) Tier-C rejection: unknown feature key → INVALID_ARGUMENT
  c) Missing required feature → INVALID_ARGUMENT
"""
from __future__ import annotations

import time
from unittest.mock import patch

import grpc
import grpc.aio
import pytest

from ml_inference_svc.grpc_stubs import (
    PredictCaseOutcomeRequest,
    PredictCaseOutcomeResponse,
    InferenceServiceStub,
    add_InferenceServiceServicer_to_server,
)
from ml_inference_svc.grpc_server import _InferenceServicer

# A minimal feature set that satisfies ALLOWLIST_FEATURES exactly.
_VALID_FEATURE_IDS = [
    "judge_severity:0.6",
    "attorney_win_rate:0.7",
    "ideology_distance:0.3",
    "materiality_score:0.8",
    "procedural_motion_count:2",
    "case_type:civil",
    "jurisdiction:Federal",
]

# Mocked return value from predict_case_outcome.
_MOCK_PREDICTION = (0.72, 0.55, 0.85, "mock-run-id-abc123")


@pytest.fixture()
async def grpc_stub():
    """
    Spin up an in-process gRPC server on a random free port.
    Yields an InferenceServiceStub connected to it.
    Tears down the server after the test.
    """
    server = grpc.aio.server()
    add_InferenceServiceServicer_to_server(_InferenceServicer(), server)
    # Port 0 lets the OS pick a free port.
    port = server.add_insecure_port("127.0.0.1:0")
    await server.start()

    channel = grpc.aio.insecure_channel(f"127.0.0.1:{port}")
    stub = InferenceServiceStub(channel)

    yield stub

    await channel.close()
    await server.stop(grace=0)


# ── a) Happy path ─────────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_predict_happy_path(grpc_stub: InferenceServiceStub):
    """Valid features → response with non-zero p_win and mlflow_run_id set."""
    with patch(
        "ml_inference_svc.grpc_server.predict_case_outcome",
        return_value=_MOCK_PREDICTION,
    ):
        response: PredictCaseOutcomeResponse = await grpc_stub.PredictCaseOutcome(
            PredictCaseOutcomeRequest(
                case_id="case-001",
                feature_ids=_VALID_FEATURE_IDS,
            )
        )

    assert response.case_id == "case-001"
    assert response.p_win > 0.0
    assert response.p_win <= 1.0
    assert response.conformal_interval.lower >= 0.0
    assert response.conformal_interval.upper <= 1.0
    assert response.conformal_interval.lower <= response.p_win
    assert response.mlflow_run_id == "mock-run-id-abc123"
    assert response.predicted_at_unix > 0


# ── b) Tier-C rejection ───────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_tier_c_feature_rejected(grpc_stub: InferenceServiceStub):
    """A feature key outside the Tier-A/B allowlist → INVALID_ARGUMENT."""
    with pytest.raises(grpc.aio.AioRpcError) as exc_info:
        await grpc_stub.PredictCaseOutcome(
            PredictCaseOutcomeRequest(
                case_id="case-002",
                feature_ids=_VALID_FEATURE_IDS + ["party_net_worth:9999999"],
            )
        )

    assert exc_info.value.code() == grpc.StatusCode.INVALID_ARGUMENT
    assert "Tier-A/B allowlist" in exc_info.value.details()


# ── c) Missing required feature ───────────────────────────────────────────────

@pytest.mark.asyncio
async def test_missing_required_feature(grpc_stub: InferenceServiceStub):
    """Omitting a required Tier-A/B feature → INVALID_ARGUMENT."""
    # Remove judge_severity from the feature list.
    incomplete = [f for f in _VALID_FEATURE_IDS if not f.startswith("judge_severity")]

    with pytest.raises(grpc.aio.AioRpcError) as exc_info:
        await grpc_stub.PredictCaseOutcome(
            PredictCaseOutcomeRequest(
                case_id="case-003",
                feature_ids=incomplete,
            )
        )

    assert exc_info.value.code() == grpc.StatusCode.INVALID_ARGUMENT
    assert "Missing required feature" in exc_info.value.details()
