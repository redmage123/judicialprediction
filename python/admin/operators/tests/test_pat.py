"""
Tests for PersonalAccessToken — S6.15.

Coverage:
- mint() generates a `pat_` + 32 hex token and stores its SHA-256 hash.
- mint() returns the plaintext exactly once; subsequent fetches see only the hash.
- hash_pat() is deterministic and 64 chars.
- revoke() flips revoked_at and is idempotent.
- is_active honors revoked_at and expires_at.
- mint_pat management command prints the plaintext and refuses super operators.
"""

from __future__ import annotations

import io
import re
import uuid
from datetime import timedelta

import pytest
from django.core.management import CommandError, call_command
from django.utils import timezone

from operators.models import Operator
from operators.pat_models import (
    PAT_PREFIX,
    PersonalAccessToken,
    hash_pat,
)


def _make_admin_operator(email: str = "pat-test@example.test") -> Operator:
    return Operator.objects.create(
        email=email,
        password_hash="bcrypt-stub",  # not exercised here
        role="admin",
        tenant_id=uuid.UUID("00000000-0000-0000-0000-000000000001"),
    )


def _make_super_operator() -> Operator:
    return Operator.objects.create(
        email="super@example.test",
        password_hash="bcrypt-stub",
        role="super",
        tenant_id=None,
    )


@pytest.mark.django_db
def test_hash_pat_is_deterministic_and_64_chars() -> None:
    a = hash_pat("pat_deadbeef")
    b = hash_pat("pat_deadbeef")
    assert a == b
    assert len(a) == 64
    assert re.fullmatch(r"[0-9a-f]{64}", a)


@pytest.mark.django_db
def test_mint_returns_plaintext_with_pat_prefix_and_stores_hash() -> None:
    op = _make_admin_operator()
    instance, plaintext = PersonalAccessToken.objects.mint(op, name="CI token")

    assert plaintext.startswith(PAT_PREFIX)
    assert len(plaintext) == len(PAT_PREFIX) + 32  # 16 random bytes → 32 hex chars
    assert instance.token_hash == hash_pat(plaintext)
    assert instance.operator_id == op.id
    assert instance.name == "CI token"
    assert instance.revoked_at is None


@pytest.mark.django_db
def test_mint_plaintext_is_not_recoverable_from_the_row() -> None:
    op = _make_admin_operator()
    instance, plaintext = PersonalAccessToken.objects.mint(op, name="x")
    fetched = PersonalAccessToken.objects.get(id=instance.id)
    # The model carries only the hash — the plaintext is gone.
    assert plaintext not in (
        fetched.token_hash,
        fetched.name,
        str(fetched),
    )


@pytest.mark.django_db
def test_revoke_flips_revoked_at_and_is_idempotent() -> None:
    op = _make_admin_operator()
    instance, _ = PersonalAccessToken.objects.mint(op, name="x")
    assert PersonalAccessToken.objects.revoke(instance.id) is True
    instance.refresh_from_db()
    assert instance.revoked_at is not None
    # Second revoke is a no-op (already revoked).
    assert PersonalAccessToken.objects.revoke(instance.id) is False


@pytest.mark.django_db
def test_is_active_honors_expires_at_and_revoked_at() -> None:
    op = _make_admin_operator()
    active, _ = PersonalAccessToken.objects.mint(op, name="active")
    assert active.is_active is True

    expired, _ = PersonalAccessToken.objects.mint(op, name="expired")
    expired.expires_at = timezone.now() - timedelta(hours=1)
    expired.save(update_fields=["expires_at"])
    assert expired.is_active is False

    revoked, _ = PersonalAccessToken.objects.mint(op, name="revoked")
    PersonalAccessToken.objects.revoke(revoked.id)
    revoked.refresh_from_db()
    assert revoked.is_active is False


@pytest.mark.django_db
def test_mint_pat_command_prints_plaintext_once() -> None:
    op = _make_admin_operator()
    out = io.StringIO()
    call_command("mint_pat", "--email", op.email, "--name", "smoke", stdout=out)
    output = out.getvalue()
    assert PAT_PREFIX in output

    # The plaintext line is the only line that starts with `pat_`.
    pat_lines = [line for line in output.splitlines() if line.startswith(PAT_PREFIX)]
    assert len(pat_lines) == 1

    # Its hash matches the stored row.
    plaintext = pat_lines[0].strip()
    instance = PersonalAccessToken.objects.get(name="smoke", operator=op)
    assert instance.token_hash == hash_pat(plaintext)


@pytest.mark.django_db
def test_mint_pat_command_refuses_super_operators() -> None:
    op = _make_super_operator()
    with pytest.raises(CommandError, match="super"):
        call_command("mint_pat", "--email", op.email, "--name", "should-fail")
    assert not PersonalAccessToken.objects.filter(operator=op).exists()
