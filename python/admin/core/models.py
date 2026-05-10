"""
Django model stubs for JudicialPredict application tables.

All models use ``managed = False`` so Django's migration runner never
attempts to CREATE, ALTER, or DROP these tables — the Rust migration system
in ``rust/feature-store/migrations/`` owns the schema.

Table structures mirror the baseline migration (20260507120000_baseline.sql).
"""

import uuid

from django.db import models


class Tenant(models.Model):
    """
    A client organisation.

    Maps to ``public.tenants`` (managed by Rust migrations).
    RLS policy: row visible only when ``app.current_tenant_id`` matches ``id``.
    The admin console connects as a superuser (Sprint-3: jp_admin with BYPASSRLS)
    so all rows are visible.
    """

    id = models.UUIDField(primary_key=True, default=uuid.uuid4, editable=False)
    slug = models.TextField(unique=True)
    name = models.TextField()
    settings = models.JSONField(default=dict)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        managed = False
        db_table = "tenants"
        verbose_name = "Tenant"
        verbose_name_plural = "Tenants"
        ordering = ["name"]

    def __str__(self) -> str:
        return f"{self.name} ({self.slug})"


class Case(models.Model):
    """
    A legal case belonging to a tenant.

    Maps to ``public.cases`` (managed by Rust migrations).
    RLS policy: row visible only when ``tenant_id`` matches ``app.current_tenant_id``.
    """

    id = models.UUIDField(primary_key=True, default=uuid.uuid4, editable=False)
    tenant = models.ForeignKey(
        Tenant,
        on_delete=models.CASCADE,
        db_column="tenant_id",
        related_name="cases",
    )
    title = models.TextField()
    jurisdiction = models.TextField()
    court = models.TextField(blank=True, null=True)
    judge_name = models.TextField(blank=True, null=True)
    parties = models.JSONField(default=dict)
    claims = models.JSONField(default=list)
    # S4.1 (JP-55): prediction persistence columns — nullable so pre-S4.2 rows stay valid.
    # Shape validation is the api-gateway resolver's responsibility (S4.2), not Django's.
    input_features = models.JSONField(null=True, blank=True)
    prediction = models.JSONField(null=True, blank=True)
    recommendation = models.JSONField(null=True, blank=True)
    created_by = models.UUIDField(null=True, blank=True)
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        managed = False
        db_table = "cases"
        verbose_name = "Case"
        verbose_name_plural = "Cases"
        ordering = ["-created_at"]

    def __str__(self) -> str:
        return self.title


class User(models.Model):
    """
    A user account scoped to a tenant.

    Maps to ``public.users``.

    .. warning::
        The ``users`` table does NOT exist in the baseline Rust migration.
        A Rust migration must be added (Sprint-3) before this model is usable.
        This stub is here so Sprint-3 can add RBAC fields and wire up SSO
        without touching the Django model definition.

    Sprint-3 TODO:
        - Write ``rust/feature-store/migrations/<ts>_create_users.sql``.
        - Define the full schema (email, role enum, sso_sub, etc.).
        - Register the table in the RLS policy.
        - Add a unique index on (tenant_id, email).
    """

    id = models.UUIDField(primary_key=True, default=uuid.uuid4, editable=False)
    tenant = models.ForeignKey(
        Tenant,
        on_delete=models.CASCADE,
        db_column="tenant_id",
        related_name="users",
    )
    email = models.EmailField(unique=True)
    # Role values: "admin", "member", "viewer" (Sprint-3 defines the full enum).
    role = models.TextField(default="member")
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        managed = False
        db_table = "users"
        verbose_name = "User"
        verbose_name_plural = "Users"
        ordering = ["email"]

    def __str__(self) -> str:
        return self.email
