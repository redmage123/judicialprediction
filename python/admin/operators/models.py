"""
Operator model — Django-managed RBAC table for the admin console.

Unlike the ``core`` models (Tenant, Case, User) this table IS managed by
Django migrations.  It lives alongside the Rust-owned application tables in
the same Postgres database.

Role semantics
--------------
admin   Read + write within their scoped tenant.  ``tenant_id`` required.
viewer  Read-only within their scoped tenant.  ``tenant_id`` required.
super   Read + write across ALL tenants via the ``jp_admin`` BYPASSRLS role.
        ``tenant_id`` MUST be NULL (enforced by the ``super_implies_null_tenant``
        check constraint).

Connection routing (ADR-003)
-----------------------------
The ``RLSMiddleware`` inspects ``request.user.email``, looks up the matching
``Operator`` row, and sets:
    - role='admin'/'viewer': sets ``app.current_tenant_id`` on the ``default``
      (jp_app) connection so RLS scopes the query.
    - role='super': routes the request to the ``admin_super`` (jp_admin)
      DATABASES alias, which carries BYPASSRLS and sees all rows.

Sprint-4 follow-ups
-------------------
- Self-service operator onboarding (SSO claim → auto-provision).
- Audit trail for Operator create/update/deactivate.
- Time-bounded elevated access (role='super' with expiry).
"""

import datetime
import secrets
import uuid

from django.contrib.auth.hashers import check_password as _check_password
from django.contrib.auth.hashers import make_password as _make_password
from django.db import models
from django.utils import timezone


class Operator(models.Model):
    ROLE_ADMIN = "admin"
    ROLE_VIEWER = "viewer"
    ROLE_SUPER = "super"

    ROLE_CHOICES = [
        (ROLE_ADMIN, "Admin"),
        (ROLE_VIEWER, "Viewer"),
        (ROLE_SUPER, "Super"),
    ]

    id = models.UUIDField(primary_key=True, default=uuid.uuid4, editable=False)
    email = models.EmailField(unique=True, help_text="Must match the Django auth user email.")
    # Optional alphanumeric alias — operators can sign in with either email or
    # username (auth_backends.py resolves both via case-insensitive Q lookup).
    username = models.CharField(
        max_length=64,
        unique=True,
        null=True,
        blank=True,
        help_text=(
            "Optional alphanumeric login alias. Operators may sign in with "
            "either email or username."
        ),
    )
    # Bcrypt hash — set via set_password(); blank means no password provisioned.
    password = models.CharField(
        max_length=128,
        blank=True,
        default="",
        help_text="Bcrypt hash.  Set via set_password(); blank = no login allowed.",
    )
    # NULL for super operators (workspace-wide); required for admin/viewer.
    tenant_id = models.UUIDField(
        null=True,
        blank=True,
        help_text="Scoped tenant UUID.  NULL only for role='super'.",
    )
    role = models.CharField(max_length=10, choices=ROLE_CHOICES, default=ROLE_VIEWER)
    is_active = models.BooleanField(default=True)
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        db_table = "operators_operator"
        ordering = ["email"]
        verbose_name = "Operator"
        verbose_name_plural = "Operators"
        constraints = [
            # role='super' operators are workspace-wide: tenant_id must be NULL.
            # role='admin'/'viewer' may have any tenant_id (including NULL during
            # creation, but the seed command always sets it).
            models.CheckConstraint(
                check=~models.Q(role="super") | models.Q(tenant_id__isnull=True),
                name="super_implies_null_tenant",
            ),
        ]

    def __str__(self) -> str:
        scope = str(self.tenant_id) if self.tenant_id else "ALL"
        return f"{self.email} [{self.role}] tenant={scope}"

    @property
    def is_super(self) -> bool:
        return self.role == self.ROLE_SUPER

    @property
    def can_write(self) -> bool:
        return self.role in (self.ROLE_ADMIN, self.ROLE_SUPER)

    # ------------------------------------------------------------------
    # Password helpers — mirrors AbstractBaseUser API so callers are clear.
    # The hash is stored on this model, NOT on Django's auth.User.
    # ------------------------------------------------------------------

    def set_password(self, raw_password: str) -> None:
        """Hash *raw_password* via PASSWORD_HASHERS and store it."""
        self.password = _make_password(raw_password)

    def check_password(self, raw_password: str) -> bool:
        """Return True if *raw_password* matches the stored hash."""
        if not self.password:
            return False
        return _check_password(raw_password, self.password)


# ---------------------------------------------------------------------------
# Password-reset token (S5.9)
#
# Replaces the S4.8 "contact your admin" stub.  A request endpoint issues
# a single-use token with a 1-hour TTL; a confirm endpoint consumes it and
# sets a new password on the linked operator.
#
# Security
# --------
# - `token` is 256 bits of CSPRNG output, URL-safe base64.  We store the
#   raw token (not a hash) because:
#     1. it's single-use and short-lived (1h);
#     2. the table is RLS-locked to the admin connection;
#     3. compromise of the DB at rest is a much larger problem than this.
# - `used_at` marks a token as redeemed so it cannot be replayed.
# - `created_at` lets us audit reset activity even after expiry / use.
# ---------------------------------------------------------------------------


PASSWORD_RESET_TOKEN_TTL_MINUTES = 60


def _generate_reset_token() -> str:
    """Return 256 bits of URL-safe CSPRNG output as the token value."""
    return secrets.token_urlsafe(32)


class PasswordResetToken(models.Model):
    id = models.UUIDField(primary_key=True, default=uuid.uuid4, editable=False)
    operator = models.ForeignKey(
        Operator,
        on_delete=models.CASCADE,
        related_name="password_reset_tokens",
    )
    token = models.CharField(
        max_length=64,
        unique=True,
        default=_generate_reset_token,
        help_text="URL-safe CSPRNG value embedded in the reset link.",
    )
    created_at = models.DateTimeField(auto_now_add=True)
    expires_at = models.DateTimeField()
    used_at = models.DateTimeField(null=True, blank=True)

    class Meta:
        db_table = "operators_passwordresettoken"
        ordering = ["-created_at"]
        verbose_name = "Password reset token"
        verbose_name_plural = "Password reset tokens"
        indexes = [
            # Hot path: confirm-endpoint lookup by token string.
            models.Index(fields=["token"], name="passwordresettoken_token_idx"),
        ]

    def __str__(self) -> str:  # pragma: no cover — admin display only
        return f"reset[{self.operator.email}] expires={self.expires_at.isoformat()}"

    # --- helpers ------------------------------------------------------------

    @classmethod
    def issue_for(cls, operator: Operator) -> "PasswordResetToken":
        """Create and persist a fresh 1h-TTL token for *operator*."""
        ttl = datetime.timedelta(minutes=PASSWORD_RESET_TOKEN_TTL_MINUTES)
        return cls.objects.create(
            operator=operator,
            expires_at=timezone.now() + ttl,
        )

    @property
    def is_expired(self) -> bool:
        return timezone.now() >= self.expires_at

    @property
    def is_used(self) -> bool:
        return self.used_at is not None

    @property
    def is_valid(self) -> bool:
        return not self.is_expired and not self.is_used

    def mark_used(self) -> None:
        """Stamp `used_at` so the token cannot be replayed."""
        self.used_at = timezone.now()
        self.save(update_fields=["used_at"])
