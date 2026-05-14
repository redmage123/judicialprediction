"""
OIDC SSO tests for S6.6 — config probe, env-gating, and the callback flow.

The Authlib client is mocked everywhere: these tests never touch a real
IdP.  DB-backed tests carry @pytest.mark.django_db; the pure-helper and
disabled-path tests run without a database.

Test matrix
-----------
test_email_from_userinfo_*          Pure claim-extraction helper.
test_sso_config_disabled            OIDC off  → {enabled: false}.
test_sso_config_enabled             OIDC on   → {enabled, provider_name}.
test_sso_login_disabled_404         OIDC off  → sso_login 404s.
test_sso_callback_disabled_404      OIDC off  → sso_callback 404s.
test_sso_login_redirects_to_idp     OIDC on   → 302 to the IdP authorize URL.
test_sso_callback_exchange_failure  Token exchange raises → 302 ?sso_error.
test_sso_callback_no_email          No email claim         → 302 ?sso_error.
test_sso_callback_unknown_operator  Email has no Operator   → 302 ?sso_error.
test_sso_callback_success           Happy path → 302 home + jp_session cookie
                                    whose JWT matches password-login claims.
"""

import uuid
from unittest import mock

import jwt
import pytest
from django.test import Client, override_settings

from operators.models import Operator
from operators.oidc import email_from_userinfo, reset_oidc_client_for_tests

_TENANT_UUID = uuid.UUID("00000000-0000-0000-0000-000000000001")
_JWT_SECRET = "dev-only-NOT-A-REAL-SECRET-1234567890abcdef"

_CONFIG_URL = "/api/auth/sso/config"
_LOGIN_URL = "/api/auth/sso/login"
_CALLBACK_URL = "/api/auth/sso/callback"

# Settings that flip OIDC on for the env-gated tests.
_OIDC_ON = dict(
    OIDC_ENABLED=True,
    OIDC_CLIENT_ID="jp-client",
    OIDC_CLIENT_SECRET="jp-secret",
    OIDC_DISCOVERY_URL="https://idp.example.test/.well-known/openid-configuration",
    OIDC_PROVIDER_NAME="Example IdP",
    WEB_BASE_URL="http://localhost:3030",
)


@pytest.fixture(autouse=True)
def _clear_oidc_cache():
    """Drop any cached Authlib client so each test sees fresh settings."""
    reset_oidc_client_for_tests()
    yield
    reset_oidc_client_for_tests()


# ---------------------------------------------------------------------------
# email_from_userinfo — pure helper
# ---------------------------------------------------------------------------


def test_email_from_userinfo_from_id_token():
    token = {"userinfo": {"email": "Alice@Example.test"}}
    assert email_from_userinfo(token, None) == "alice@example.test"


def test_email_from_userinfo_falls_back_to_userinfo_endpoint():
    token = {}  # IdP put nothing in the id_token
    userinfo = {"email": "bob@example.test"}
    assert email_from_userinfo(token, userinfo) == "bob@example.test"


def test_email_from_userinfo_rejects_unverified():
    token = {"userinfo": {"email": "c@example.test", "email_verified": False}}
    assert email_from_userinfo(token, None) is None


def test_email_from_userinfo_none_when_missing():
    assert email_from_userinfo({}, None) is None
    assert email_from_userinfo({"userinfo": {}}, {}) is None


# ---------------------------------------------------------------------------
# /api/auth/sso/config
# ---------------------------------------------------------------------------


def test_sso_config_disabled():
    """Default settings: OIDC off → {enabled: false}, always 200."""
    res = Client().get(_CONFIG_URL)
    assert res.status_code == 200
    assert res.json() == {"enabled": False}


@override_settings(**_OIDC_ON)
def test_sso_config_enabled():
    res = Client().get(_CONFIG_URL)
    assert res.status_code == 200
    assert res.json() == {"enabled": True, "provider_name": "Example IdP"}


# ---------------------------------------------------------------------------
# Env-gating: endpoints 404 when OIDC is disabled
# ---------------------------------------------------------------------------


def test_sso_login_disabled_404():
    res = Client().get(_LOGIN_URL)
    assert res.status_code == 404
    assert res.json() == {"ok": False, "error": "sso_disabled"}


def test_sso_callback_disabled_404():
    res = Client().get(_CALLBACK_URL, {"code": "x", "state": "y"})
    assert res.status_code == 404
    assert res.json() == {"ok": False, "error": "sso_disabled"}


# ---------------------------------------------------------------------------
# /api/auth/sso/login — initiates the flow
# ---------------------------------------------------------------------------


@override_settings(**_OIDC_ON)
def test_sso_login_redirects_to_idp():
    """sso_login delegates to the Authlib client's authorize_redirect."""
    from django.http import HttpResponseRedirect

    fake_client = mock.Mock()
    fake_client.authorize_redirect.return_value = HttpResponseRedirect(
        "https://idp.example.test/authorize?client_id=jp-client&state=abc"
    )
    with mock.patch(
        "operators.oidc_views.get_oidc_client", return_value=fake_client
    ):
        res = Client().get(_LOGIN_URL)

    assert res.status_code == 302
    assert res["Location"].startswith("https://idp.example.test/authorize")
    # The callback URL handed to the IdP must point at the web BFF proxy.
    args, _ = fake_client.authorize_redirect.call_args
    assert args[1] == "http://localhost:3030/api/auth/sso/callback"


# ---------------------------------------------------------------------------
# /api/auth/sso/callback — failure paths
# ---------------------------------------------------------------------------


@override_settings(**_OIDC_ON)
def test_sso_callback_exchange_failure():
    """Token exchange raising → 302 to /login?sso_error=exchange_failed."""
    fake_client = mock.Mock()
    fake_client.authorize_access_token.side_effect = RuntimeError("bad code")
    with mock.patch(
        "operators.oidc_views.get_oidc_client", return_value=fake_client
    ):
        res = Client().get(_CALLBACK_URL, {"code": "x", "state": "y"})

    assert res.status_code == 302
    assert res["Location"] == "http://localhost:3030/login?sso_error=exchange_failed"


@override_settings(**_OIDC_ON)
def test_sso_callback_no_email():
    """A token without an email claim → 302 ?sso_error=no_email."""
    fake_client = mock.Mock()
    fake_client.authorize_access_token.return_value = {"userinfo": {"sub": "abc"}}
    with mock.patch(
        "operators.oidc_views.get_oidc_client", return_value=fake_client
    ):
        res = Client().get(_CALLBACK_URL, {"code": "x", "state": "y"})

    assert res.status_code == 302
    assert res["Location"] == "http://localhost:3030/login?sso_error=no_email"


@pytest.mark.django_db
@override_settings(**_OIDC_ON)
def test_sso_callback_unknown_operator():
    """Verified email with no matching Operator → 302 ?sso_error=unknown_operator."""
    fake_client = mock.Mock()
    fake_client.authorize_access_token.return_value = {
        "userinfo": {"email": "nobody@example.test", "email_verified": True}
    }
    with mock.patch(
        "operators.oidc_views.get_oidc_client", return_value=fake_client
    ):
        res = Client().get(_CALLBACK_URL, {"code": "x", "state": "y"})

    assert res.status_code == 302
    assert res["Location"] == (
        "http://localhost:3030/login?sso_error=unknown_operator"
    )
    assert "jp_session" not in res.cookies


# ---------------------------------------------------------------------------
# /api/auth/sso/callback — happy path
# ---------------------------------------------------------------------------


@pytest.mark.django_db
@override_settings(**_OIDC_ON)
def test_sso_callback_success():
    """Happy path: existing operator → 302 home + jp_session JWT cookie."""
    operator = Operator(
        email="sso-test@example.test",
        role=Operator.ROLE_ADMIN,
        tenant_id=_TENANT_UUID,
        is_active=True,
    )
    # SSO operators need no password — the column stays blank.
    operator.save()

    fake_client = mock.Mock()
    fake_client.authorize_access_token.return_value = {
        "userinfo": {"email": "sso-test@example.test", "email_verified": True}
    }
    with mock.patch(
        "operators.oidc_views.get_oidc_client", return_value=fake_client
    ):
        res = Client().get(_CALLBACK_URL, {"code": "x", "state": "y"})

    # Redirected to the web app home.
    assert res.status_code == 302
    assert res["Location"] == "http://localhost:3030/"

    # jp_session cookie set, carrying a JWT with the SAME claims password
    # login would have minted.
    assert "jp_session" in res.cookies
    token = res.cookies["jp_session"].value
    claims = jwt.decode(
        token, _JWT_SECRET, algorithms=["HS256"], audience="judicialpredict-api"
    )
    assert claims["sub"] == str(operator.id)
    assert claims["tenant_id"] == str(_TENANT_UUID)
    assert claims["role"] == Operator.ROLE_ADMIN
    assert claims["iss"] == "judicialpredict-admin"
    assert claims["aud"] == "judicialpredict-api"


@pytest.mark.django_db
@override_settings(**_OIDC_ON)
def test_sso_callback_inactive_operator_rejected():
    """An email matching an inactive operator is treated as unknown."""
    operator = Operator(
        email="inactive-sso@example.test",
        role=Operator.ROLE_VIEWER,
        tenant_id=_TENANT_UUID,
        is_active=False,
    )
    operator.save()

    fake_client = mock.Mock()
    fake_client.authorize_access_token.return_value = {
        "userinfo": {"email": "inactive-sso@example.test", "email_verified": True}
    }
    with mock.patch(
        "operators.oidc_views.get_oidc_client", return_value=fake_client
    ):
        res = Client().get(_CALLBACK_URL, {"code": "x", "state": "y"})

    assert res.status_code == 302
    assert res["Location"].endswith("sso_error=unknown_operator")
    assert "jp_session" not in res.cookies
