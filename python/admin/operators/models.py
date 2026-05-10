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

import uuid

from django.contrib.auth.hashers import check_password as _check_password
from django.contrib.auth.hashers import make_password as _make_password
from django.db import models


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
