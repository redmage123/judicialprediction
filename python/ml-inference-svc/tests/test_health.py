"""
Health endpoint tests — liveness and readiness probes.
Uses httpx.AsyncClient against the FastAPI app directly (no network).
"""

import pytest
from httpx import ASGITransport, AsyncClient

from ml_inference_svc.main import app


@pytest.mark.asyncio
async def test_healthz_returns_200_and_ok_body() -> None:
    async with AsyncClient(
        transport=ASGITransport(app=app), base_url="http://test"
    ) as client:
        resp = await client.get("/healthz")

    assert resp.status_code == 200
    body = resp.json()
    assert body == {"status": "ok"}, f"unexpected body: {body}"


@pytest.mark.asyncio
async def test_readyz_returns_200() -> None:
    async with AsyncClient(
        transport=ASGITransport(app=app), base_url="http://test"
    ) as client:
        resp = await client.get("/readyz")

    assert resp.status_code == 200
    body = resp.json()
    assert body.get("status") == "ready", f"unexpected body: {body}"
