"""
ml-inference-svc entry point.

Exposes:
  GET /healthz — liveness probe (always 200 if process is alive)
  GET /readyz  — readiness probe (placeholder; returns 200 for now)

Real prediction endpoints and gRPC server are added in Sprint M3.
"""

from __future__ import annotations

import uvicorn
from fastapi import FastAPI
from fastapi.responses import JSONResponse

app = FastAPI(
    title="JudicialPredict ML Inference Service",
    version="0.1.0",
    description="Python ML plane — case-outcome predictions, SHAP, conformal intervals.",
)


@app.get("/healthz", response_class=JSONResponse, tags=["ops"])
async def healthz() -> dict[str, str]:
    """Liveness probe — returns 200 as long as the process is alive."""
    return {"status": "ok"}


@app.get("/readyz", response_class=JSONResponse, tags=["ops"])
async def readyz() -> dict[str, str]:
    """
    Readiness probe — placeholder.
    Sprint M3: will check model weights loaded + feature-store gRPC reachable.
    """
    return {"status": "ready"}


if __name__ == "__main__":
    uvicorn.run("ml_inference_svc.main:app", host="0.0.0.0", port=8001, reload=False)
