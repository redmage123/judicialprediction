"""
Password-reset tests for S5.9 — request + confirm endpoints.

Coverage matrix
---------------
test_request_known_email_sends_email   Happy path — 200, mail sent, token created.
test_request_unknown_email_no_leak     Unknown email → 200 (same shape), no mail.
test_request_inactive_operator         Inactive operator → 200, no mail.
test_confirm_valid_token_updates       Happy path — 200, password rotated, token marked used.
test_confirm_invalid_token             Unknown token → 400 invalid_token.
test_confirm_expired_token             Past expires_at → 400 invalid_token.
test_confirm_used_token                used_at set → 400 invalid_token (no replay).
test_confirm_weak_password             Fails Django password validator → 400 weak_password.
test_confirm_missing_fields            Empty token / new_password → 400 invalid_request.
"""

import datetime
import json
import uuid

import pytest
from django.core import mail
from django.test import Client
from django.utils import timezone

from operators.models import (
    PASSWORD_RESET_TOKEN_TTL_MINUTES,
    Operator,
    PasswordResetToken,
)

_TENANT_UUID = uuid.UUID("00000000-0000-0000-0000-000000000001")
_REQUEST_URL = "/api/auth/reset/request"
_CONFIRM_URL = "/api/auth/reset/confirm"


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def operator(db):
    """Active admin operator with a known starting password."""
    op = Operator(
        email="reset-test@example.test",
        role=Operator.ROLE_ADMIN,
        tenant_id=_TENANT_UUID,
        is_active=True,
    )
    op.set_password("starting-password-9!ZqXp")
    op.save()
    return op


@pytest.fixture
def inactive_operator(db):
    """Deactivated operator — must not receive reset emails."""
    op = Operator(
        email="reset-inactive@example.test",
        role=Operator.ROLE_VIEWER,
        tenant_id=_TENANT_UUID,
        is_active=False,
    )
    op.set_password("ignored-secret-9!ZqXp")
    op.save()
    return op


def _post(client: Client, url: str, payload: dict):
    return client.post(url, data=json.dumps(payload), content_type="application/json")


# ---------------------------------------------------------------------------
# /api/auth/reset/request
# ---------------------------------------------------------------------------


@pytest.mark.django_db
def test_request_known_email_sends_email(operator, settings):
    settings.EMAIL_BACKEND = "django.core.mail.backends.locmem.EmailBackend"
    mail.outbox.clear()

    res = _post(Client(), _REQUEST_URL, {"email": operator.email})

    assert res.status_code == 200
    body = res.json()
    assert body["ok"] is True
    assert body["ttl_minutes"] == PASSWORD_RESET_TOKEN_TTL_MINUTES

    assert len(mail.outbox) == 1
    sent = mail.outbox[0]
    assert sent.to == [operator.email]
    assert "password reset" in sent.subject.lower()

    # Exactly one fresh token should exist, and the email body must
    # contain it.
    tokens = list(PasswordResetToken.objects.filter(operator=operator))
    assert len(tokens) == 1
    assert tokens[0].token in sent.body
    assert tokens[0].is_valid


@pytest.mark.django_db
def test_request_unknown_email_no_leak(db, settings):
    """Unknown email returns the same shape — no enumeration possible."""
    settings.EMAIL_BACKEND = "django.core.mail.backends.locmem.EmailBackend"
    mail.outbox.clear()

    res = _post(Client(), _REQUEST_URL, {"email": "nobody@nowhere.test"})

    assert res.status_code == 200
    assert res.json()["ok"] is True
    assert mail.outbox == []
    assert PasswordResetToken.objects.count() == 0


@pytest.mark.django_db
def test_request_inactive_operator(inactive_operator, settings):
    """Inactive operator: same 200 response, but no email and no token."""
    settings.EMAIL_BACKEND = "django.core.mail.backends.locmem.EmailBackend"
    mail.outbox.clear()

    res = _post(Client(), _REQUEST_URL, {"email": inactive_operator.email})

    assert res.status_code == 200
    assert mail.outbox == []
    assert PasswordResetToken.objects.filter(operator=inactive_operator).count() == 0


# ---------------------------------------------------------------------------
# /api/auth/reset/confirm
# ---------------------------------------------------------------------------


@pytest.mark.django_db
def test_confirm_valid_token_updates(operator):
    token = PasswordResetToken.issue_for(operator)
    res = _post(
        Client(),
        _CONFIRM_URL,
        {"token": token.token, "new_password": "rotated-password-7$Qj"},
    )

    assert res.status_code == 200, res.content
    assert res.json() == {"ok": True}

    operator.refresh_from_db()
    assert operator.check_password("rotated-password-7$Qj")
    assert not operator.check_password("starting-password-9!ZqXp")

    token.refresh_from_db()
    assert token.used_at is not None
    assert not token.is_valid


@pytest.mark.django_db
def test_confirm_invalid_token(operator):
    res = _post(
        Client(),
        _CONFIRM_URL,
        {"token": "not-a-real-token", "new_password": "rotated-password-7$Qj"},
    )
    assert res.status_code == 400
    assert res.json()["error"] == "invalid_token"


@pytest.mark.django_db
def test_confirm_expired_token(operator):
    token = PasswordResetToken.issue_for(operator)
    # Force expiry by rewinding expires_at.
    token.expires_at = timezone.now() - datetime.timedelta(minutes=1)
    token.save(update_fields=["expires_at"])

    res = _post(
        Client(),
        _CONFIRM_URL,
        {"token": token.token, "new_password": "rotated-password-7$Qj"},
    )
    assert res.status_code == 400
    assert res.json()["error"] == "invalid_token"

    operator.refresh_from_db()
    # Password must NOT have been rotated.
    assert operator.check_password("starting-password-9!ZqXp")


@pytest.mark.django_db
def test_confirm_used_token(operator):
    token = PasswordResetToken.issue_for(operator)
    token.mark_used()

    res = _post(
        Client(),
        _CONFIRM_URL,
        {"token": token.token, "new_password": "rotated-password-7$Qj"},
    )
    assert res.status_code == 400
    assert res.json()["error"] == "invalid_token"


@pytest.mark.django_db
def test_confirm_weak_password(operator):
    token = PasswordResetToken.issue_for(operator)

    # "password" is in Django's common-passwords list AND below 8 chars in
    # the default MinimumLengthValidator's threshold for some configurations;
    # either failure leads to weak_password.
    res = _post(
        Client(),
        _CONFIRM_URL,
        {"token": token.token, "new_password": "password"},
    )
    assert res.status_code == 400
    body = res.json()
    assert body["error"] == "weak_password"
    assert isinstance(body["details"], list) and body["details"]

    # The token must NOT have been consumed on validation failure — operator
    # gets to try again with a stronger password.
    token.refresh_from_db()
    assert token.used_at is None
    operator.refresh_from_db()
    assert operator.check_password("starting-password-9!ZqXp")


@pytest.mark.django_db
def test_confirm_missing_fields(db):
    res = _post(Client(), _CONFIRM_URL, {"token": "", "new_password": ""})
    assert res.status_code == 400
    assert res.json()["error"] == "invalid_request"
