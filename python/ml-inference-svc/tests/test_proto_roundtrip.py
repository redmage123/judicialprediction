"""
Proto round-trip tests — serialize then deserialize generated protobuf messages.
Mirrors the Rust smoke tests in feature-store/src/lib.rs to prove both planes
work from the same .proto source (ADR-002).
"""

import sys
import os

# Ensure grpc_stubs is importable even if the package isn't installed editably.
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from ml_inference_svc.grpc_stubs import (
    inference_pb2,
    feature_store_pb2,
)


def test_predict_case_outcome_request_round_trip() -> None:
    """PredictCaseOutcomeRequest serializes and deserializes identically."""
    original = inference_pb2.PredictCaseOutcomeRequest(
        case_id="case-001",
        feature_ids=["judge.reversal_rate.circuit9", "attorney.win_rate"],
        model_variant=inference_pb2.MODEL_VARIANT_XGBOOST,
        conformal_coverage=0.90,
        trace_id="trace-abc-123",
    )

    serialized = original.SerializeToString()
    assert len(serialized) > 0, "serialized bytes must be non-empty"

    decoded = inference_pb2.PredictCaseOutcomeRequest()
    decoded.ParseFromString(serialized)

    assert decoded.case_id == original.case_id
    assert list(decoded.feature_ids) == list(original.feature_ids)
    assert decoded.model_variant == original.model_variant
    assert abs(decoded.conformal_coverage - original.conformal_coverage) < 1e-9
    assert decoded.trace_id == original.trace_id


def test_get_feature_request_round_trip() -> None:
    """GetFeatureRequest serializes and deserializes identically."""
    original = feature_store_pb2.GetFeatureRequest(
        case_id="case-001",
        feature_id="judge.reversal_rate.circuit9",
        permitted_use=feature_store_pb2.PERMITTED_USE_DISPARATE_IMPACT_AUDIT,
    )

    serialized = original.SerializeToString()
    assert len(serialized) > 0

    decoded = feature_store_pb2.GetFeatureRequest()
    decoded.ParseFromString(serialized)

    assert decoded.case_id == original.case_id
    assert decoded.feature_id == original.feature_id
    assert decoded.permitted_use == original.permitted_use
