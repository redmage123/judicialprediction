"""
gRPC server for ml-inference-svc.

Serves InferenceService.PredictCaseOutcome on port 51051 (default) alongside
the existing FastAPI HTTP server.  Uses grpc.aio so it shares the uvicorn
asyncio event loop without spinning up a separate thread pool.

Protocol contract: ADR-002 / inference.proto
"""
from __future__ import annotations

import json
import logging
import os
import time

import grpc
import grpc.aio

from ml_inference_svc import audit_recorder
from ml_inference_svc.grpc_stubs import (
    PredictCaseOutcomeRequest,
    PredictCaseOutcomeResponse,
    ConformalInterval,
    add_InferenceServiceServicer_to_server,
)
from ml_inference_svc.grpc_stubs.judicialpredict.ml_plane.inference.v1 import inference_pb2_grpc
from ml_inference_svc.predict import ALLOWLIST_FEATURES, predict_case_outcome

logger = logging.getLogger(__name__)

GRPC_PORT: int = int(os.environ.get("ML_INFERENCE_GRPC_PORT", "51051"))


class _InferenceServicer(inference_pb2_grpc.InferenceServiceServicer):
    """Async implementation of InferenceService.PredictCaseOutcome."""

    async def PredictCaseOutcome(
        self,
        request: PredictCaseOutcomeRequest,
        context: grpc.aio.ServicerContext,
    ) -> PredictCaseOutcomeResponse:
        # Build feature dict from feature_ids present in the request.
        # In this sprint feature_ids carries key=value pairs encoded as
        # "key:value" strings (matching the REST /predict contract shape).
        # JP-71 will wire this to the real FeatureStore; for now we parse
        # the feature_ids list as the feature dict.
        features: dict = {}
        for fid in request.feature_ids:
            if ":" in fid:
                k, _, v = fid.partition(":")
                features[k.strip()] = v.strip()
            else:
                features[fid] = fid  # sentinel — will fail validation below

        # Tier enforcement: any field not on the allowlist is rejected.
        forbidden = set(features.keys()) - ALLOWLIST_FEATURES
        if forbidden:
            await context.abort(
                grpc.StatusCode.INVALID_ARGUMENT,
                f"Forbidden feature(s) not in Tier-A/B allowlist: {sorted(forbidden)}",
            )
            return PredictCaseOutcomeResponse()

        missing = ALLOWLIST_FEATURES - set(features.keys())
        if missing:
            await context.abort(
                grpc.StatusCode.INVALID_ARGUMENT,
                f"Missing required feature(s): {sorted(missing)}",
            )
            return PredictCaseOutcomeResponse()

        started = time.perf_counter()
        audit_status = audit_recorder.STATUS_OK

        try:
            p_win, ci_lower, ci_upper, model_version = predict_case_outcome(features)
        except FileNotFoundError as exc:
            audit_status = audit_recorder.STATUS_ERR
            await context.abort(grpc.StatusCode.UNAVAILABLE, str(exc))
            return PredictCaseOutcomeResponse()
        except Exception as exc:
            audit_status = audit_recorder.STATUS_ERR
            await context.abort(grpc.StatusCode.INTERNAL, f"Inference error: {exc}")
            return PredictCaseOutcomeResponse()
        finally:
            # Fire-and-forget audit record; pull tenant_id from gRPC metadata.
            tenant_id = _tenant_id_from_metadata(context)
            if tenant_id is not None:
                latency_ms = int((time.perf_counter() - started) * 1000)
                payload_hash = audit_recorder.hash_payload(
                    json.dumps(features, sort_keys=True).encode("utf-8")
                )
                audit_recorder.record_fire_and_forget(
                    tenant_id=tenant_id,
                    actor="ml-inference-svc-grpc",
                    action="predict.invoke",
                    payload_hash=payload_hash,
                    latency_ms=latency_ms,
                    status=audit_status,
                )

        return PredictCaseOutcomeResponse(
            case_id=request.case_id,
            p_win=p_win,
            conformal_interval=ConformalInterval(
                lower=ci_lower,
                upper=ci_upper,
                coverage=0.90,
            ),
            mlflow_run_id=model_version,
            predicted_at_unix=int(time.time()),
        )


def _tenant_id_from_metadata(context: grpc.aio.ServicerContext) -> str | None:
    """Extract x-tenant-id from gRPC request metadata, or return None."""
    try:
        meta = dict(context.invocation_metadata())
        return meta.get("x-tenant-id")
    except Exception:
        return None


async def build_grpc_server() -> grpc.aio.Server:
    """Create, configure, and start the gRPC server. Returns the started server."""
    server = grpc.aio.server()
    add_InferenceServiceServicer_to_server(_InferenceServicer(), server)
    listen_addr = f"0.0.0.0:{GRPC_PORT}"
    server.add_insecure_port(listen_addr)
    await server.start()
    logger.info("gRPC InferenceService listening on %s", listen_addr)
    return server
