"""
ml-inference-svc entry point.

Endpoints:
  GET  /healthz  — liveness probe
  GET  /readyz   — readiness probe
  POST /predict  — gradient-boosted ensemble prediction with 90 % conformal CI

Tier-C party features are NEVER accepted by /predict.
Any field outside the Tier-A/B allowlist is rejected with HTTP 400.
"""
from __future__ import annotations

import json
import time
from contextlib import asynccontextmanager
from typing import Any, Optional

import uvicorn
from fastapi import Body, FastAPI, Header, HTTPException
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field

from ml_inference_svc import audit_recorder
from ml_inference_svc.grpc_server import build_grpc_server
from ml_inference_svc.predict import ALLOWLIST_FEATURES, predict_case_outcome


@asynccontextmanager
async def _lifespan(application: FastAPI):  # noqa: ARG001
    """Start gRPC server on startup; stop it gracefully on shutdown."""
    grpc_server = await build_grpc_server()
    yield
    await grpc_server.stop(grace=5)


app = FastAPI(
    title="JudicialPredict ML Inference Service",
    version="0.2.0",
    description="Python ML plane — case-outcome predictions, SHAP, conformal intervals.",
    lifespan=_lifespan,
)


# ── ops ───────────────────────────────────────────────────────────────────────

@app.get("/healthz", response_class=JSONResponse, tags=["ops"])
async def healthz() -> dict[str, str]:
    """Liveness probe — returns 200 as long as the process is alive."""
    return {"status": "ok"}


@app.get("/readyz", response_class=JSONResponse, tags=["ops"])
async def readyz() -> dict[str, str]:
    """Readiness probe — checks that champion.json is present."""
    try:
        from ml_inference_svc.predict import _champion_meta
        _champion_meta()
        return {"status": "ready"}
    except FileNotFoundError:
        return JSONResponse(
            status_code=503,
            content={"status": "not_ready", "detail": "model not trained"},
        )


# ── prediction ────────────────────────────────────────────────────────────────

class PredictResponse(BaseModel):
    p_win: float = Field(..., ge=0.0, le=1.0, description="Calibrated win probability")
    ci_lower: float = Field(..., ge=0.0, le=1.0, description="Conformal CI lower bound")
    ci_upper: float = Field(..., ge=0.0, le=1.0, description="Conformal CI upper bound")
    coverage: float = Field(default=0.90, description="Nominal CI coverage")
    model_version: str = Field(..., description="MLflow run_id of the champion model")
    predicted_at_unix: int = Field(..., description="Epoch seconds of prediction")


@app.post("/predict", response_model=PredictResponse, tags=["ml"])
async def predict(
    body: dict[str, Any] = Body(...),
    x_tenant_id: Optional[str] = Header(default=None, alias="X-Tenant-Id"),
) -> PredictResponse:
    """
    Predict case outcome.

    Accepts JSON with Tier-A/B feature fields only.
    Rejects any field outside the allowlist with HTTP 400 to guard against Tier-C inputs.
    Returns P(win) + 90 % conformal CI per the PredictCaseOutcome proto contract.

    If X-Tenant-Id is provided, a fire-and-forget audit row is recorded in
    audit_log via audit_recorder (S2.11 / JP-34).  No header → no audit row.
    """
    # Tier enforcement: any field not on the allowlist is rejected.
    forbidden = set(body.keys()) - ALLOWLIST_FEATURES
    if forbidden:
        raise HTTPException(
            status_code=400,
            detail=f"Forbidden feature(s) not in Tier-A/B allowlist: {sorted(forbidden)}",
        )

    missing = ALLOWLIST_FEATURES - set(body.keys())
    if missing:
        raise HTTPException(
            status_code=400,
            detail=f"Missing required feature(s): {sorted(missing)}",
        )

    started = time.perf_counter()
    audit_status = audit_recorder.STATUS_OK
    try:
        p_win, ci_lower, ci_upper, model_version = predict_case_outcome(body)
    except FileNotFoundError as exc:
        audit_status = audit_recorder.STATUS_ERR
        raise HTTPException(status_code=503, detail=str(exc)) from exc
    except Exception as exc:
        audit_status = audit_recorder.STATUS_ERR
        raise HTTPException(status_code=500, detail=f"Inference error: {exc}") from exc
    finally:
        if x_tenant_id is not None:
            latency_ms = int((time.perf_counter() - started) * 1000)
            payload_hash = audit_recorder.hash_payload(
                json.dumps(body, sort_keys=True).encode("utf-8")
            )
            audit_recorder.record_fire_and_forget(
                tenant_id=x_tenant_id,
                actor="ml-inference-svc",
                action="predict.invoke",
                payload_hash=payload_hash,
                latency_ms=latency_ms,
                status=audit_status,
            )

    return PredictResponse(
        p_win=p_win,
        ci_lower=ci_lower,
        ci_upper=ci_upper,
        coverage=0.90,
        model_version=model_version,
        predicted_at_unix=int(time.time()),
    )


if __name__ == "__main__":
    uvicorn.run("ml_inference_svc.main:app", host="0.0.0.0", port=8001, reload=False)
