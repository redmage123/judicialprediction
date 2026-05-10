"""
Auth tests for S4.8 — OperatorAuthBackend + login/logout views.

Requires a live Postgres instance (pytest-django's @pytest.mark.django_db).
In CI these run against the dev stack defined in docker-compose.dev.yml.

Test matrix
-----------
test_login_success               Happy path — 200, jp_session cookie, valid JWT claims.
test_login_wrong_password        Wrong password → 401, no cookie.
test_login_unknown_email         Unknown email → 401, no cookie.
test_logout_clears_cookie        Logout → 204, cookie cleared (max-age 0 or absent).
"""

import json
import uuid

import jwt
import pytest
from django.test import Client

from operators.models import Operator

_TENANT_UUID = uuid.UUID("00000000-0000-0000-0000-000000000001")
_JWT_SECRET = "dev-only-NOT-A-REAL-SECRET-1234567890abcdef"
_LOGIN_URL = "/api/auth/login"
_LOGOUT_URL = "/api/auth/logout"


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def tenant_admin(db):
    """Active admin operator with a known bcrypt password."""
    op = Operator(
        email="auth-test-admin@example.test",
        role=Operator.ROLE_ADMIN,
        tenant_id=_TENANT_UUID,
        is_active=True,
    )
    op.set_password("correct-password")
    op.save()
    return op


def _post_login(client: Client, email: str, password: str):
    return client.post(
        _LOGIN_URL,
        data=json.dumps({"email": email, "password": password}),
        content_type="application/json",
    )


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.django_db
def test_login_success(tenant_admin):
    """POST /api/auth/login with valid creds → 200, cookie set, correct JWT claims."""
    client = Client()
    res = _post_login(client, tenant_admin.email, "correct-password")

    assert res.status_code == 200
    data = res.json()
    assert data == {"ok": True}

    # Cookie must be present.
    assert "jp_session" in res.cookies
    token = res.cookies["jp_session"].value
    assert token

    # Decode without verification (we just check claim shape; signature is
    # verified end-to-end by the Rust api-gateway in the integration stack).
    claims = jwt.decode(token, _JWT_SECRET, algorithms=["HS256"], audience="judicialpredict-api")

    assert claims["sub"] == str(tenant_admin.id)
    assert claims["tenant_id"] == str(_TENANT_UUID)
    assert claims["role"] == Operator.ROLE_ADMIN
    assert claims["iss"] == "judicialpredict-admin"
    assert claims["aud"] == "judicialpredict-api"
    # exp must be ~8 hours in the future (within a 60-second window for CI).
    import time
    assert claims["exp"] - time.time() > 8 * 3600 - 60


@pytest.mark.django_db
def test_login_wrong_password(tenant_admin):
    """Wrong password → 401, no jp_session cookie."""
    client = Client()
    res = _post_login(client, tenant_admin.email, "wrong-password")

    assert res.status_code == 401
    assert res.json() == {"ok": False, "error": "invalid_credentials"}
    assert "jp_session" not in res.cookies


@pytest.mark.django_db
def test_login_unknown_email():
    """Unknown email → 401, no jp_session cookie."""
    client = Client()
    res = _post_login(client, "nobody@example.test", "whatever")

    assert res.status_code == 401
    assert res.json() == {"ok": False, "error": "invalid_credentials"}
    assert "jp_session" not in res.cookies


@pytest.mark.django_db
def test_logout_clears_cookie(tenant_admin):
    """POST /api/auth/logout → 204 and jp_session cookie cleared."""
    client = Client()

    # Login first to get a real cookie in the client jar.
    login_res = _post_login(client, tenant_admin.email, "correct-password")
    assert login_res.status_code == 200

    # Logout.
    logout_res = client.post(_LOGOUT_URL, content_type="application/json")
    assert logout_res.status_code == 204

    # The cookie must either be absent or have max-age=0 / expired.
    if "jp_session" in logout_res.cookies:
        cookie = logout_res.cookies["jp_session"]
        max_age = cookie.get("max-age") or cookie.get("Max-Age") or "1"
        assert str(max_age) == "0" or cookie.value == ""
