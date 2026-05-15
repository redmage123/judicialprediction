"""
PersonalAccessToken — S6.15.

Maps the SQL table created by the
``20260516T000000_personal_access_tokens.sql`` migration.  Each Operator
can mint multiple PATs; the plaintext is shown to the operator exactly
once at mint time and only the SHA-256 hex hash is stored.

The Django app does NOT manage this table's DDL (``managed = False``);
the Rust feature-store migration is the source of truth.  This file
exists so the mint / verify / revoke logic lives next to the Operator
model and so a future admin UI can use the standard ``django.contrib.admin``
registration without re-implementing the lookup query.
"""

from __future__ import annotations

import hashlib
import secrets
import uuid
from typing import TYPE_CHECKING

from django.db import models
from django.utils import timezone

if TYPE_CHECKING:
    from .models import Operator

PAT_PREFIX = "pat_"
PAT_BODY_HEX_CHARS = 32  # 16 random bytes = 32 hex chars; ~128 bits of entropy.


def hash_pat(plaintext: str) -> str:
    """SHA-256 hex of the plaintext PAT.  Matches the Rust gateway's
    ``pat_auth::hash_pat_token`` exactly so the two sides agree."""
    return hashlib.sha256(plaintext.encode("utf-8")).hexdigest()


def _generate_plaintext() -> str:
    """Generate a new ``pat_<32 hex>`` token using ``secrets``."""
    return f"{PAT_PREFIX}{secrets.token_hex(PAT_BODY_HEX_CHARS // 2)}"


class PersonalAccessTokenManager(models.Manager):
    def mint(
        self,
        operator: "Operator",
        name: str,
        expires_at: timezone.datetime | None = None,
    ) -> tuple["PersonalAccessToken", str]:
        """Generate a plaintext PAT, store its hash, and return both.

        The plaintext is returned ONLY from this call site — the caller is
        responsible for showing it to the operator and discarding it
        immediately.  Subsequent lookups return only the hash."""
        plaintext = _generate_plaintext()
        token_hash = hash_pat(plaintext)
        instance = self.create(
            operator=operator,
            name=name,
            token_hash=token_hash,
            expires_at=expires_at,
        )
        return instance, plaintext

    def revoke(self, pat_id: uuid.UUID) -> bool:
        """Mark a PAT as revoked.  Idempotent; returns True if it was
        active before the call, False if it was already revoked or unknown."""
        now = timezone.now()
        updated = self.filter(id=pat_id, revoked_at__isnull=True).update(
            revoked_at=now
        )
        return updated > 0


class PersonalAccessToken(models.Model):
    id = models.UUIDField(primary_key=True, default=uuid.uuid4, editable=False)
    operator = models.ForeignKey(
        "operators.Operator",
        on_delete=models.CASCADE,
        related_name="personal_access_tokens",
    )
    name = models.CharField(max_length=255)
    token_hash = models.CharField(max_length=64, unique=True)
    created_at = models.DateTimeField(auto_now_add=True)
    last_used_at = models.DateTimeField(null=True, blank=True)
    revoked_at = models.DateTimeField(null=True, blank=True)
    expires_at = models.DateTimeField(null=True, blank=True)

    objects = PersonalAccessTokenManager()

    class Meta:
        db_table = "personal_access_tokens"
        # DDL is owned by the operators Django migration (S6.15 / 0005).
        # The Rust gateway reads (and writes last_used_at to) this table;
        # the schema source-of-truth lives here in Django.

    def __str__(self) -> str:
        suffix = "revoked" if self.revoked_at else "active"
        return f"PAT[{self.name}] {self.operator_id} ({suffix})"

    @property
    def is_active(self) -> bool:
        if self.revoked_at is not None:
            return False
        if self.expires_at is not None and self.expires_at <= timezone.now():
            return False
        return True
