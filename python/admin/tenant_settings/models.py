"""
TenantSetting model — unmanaged stub for the tenant_settings table.

The schema is owned by the Rust migration system:
  rust/feature-store/migrations/20260509120000_tenant_settings.sql

The table has a surrogate ``id`` (uuid PK) and a UNIQUE ``tenant_id`` column.
We expose ``tenant_id`` as the ORM primary key so that admin lookups use the
natural business key (one row per tenant) and the Django admin URL is the
tenant UUID rather than an internal surrogate.

``managed = False`` — Django never creates, alters, or drops this table.
"""

import uuid

from django.db import models


class TenantSetting(models.Model):
    """
    Per-tenant feature-tier override configuration.

    ``feature_tier_overrides`` jsonb shape (mirrors Rust TenantOverrides):
    ::

        {
          "disabled_features": ["attorney_win_rate"],
          "tier_overrides":    {"judge_severity": "TIER_C"}
        }

    Semantics: tightening only — overrides cannot grant features that the
    global tier policy forbids.
    """

    # Use tenant_id as the ORM primary key; the real DB pk is `id` (uuid),
    # but tenant_id is UNIQUE so Django queries work correctly.
    tenant_id = models.UUIDField(
        primary_key=True,
        default=uuid.uuid4,
        # editable=False removed: PK is auto-non-editable; readonly_fields handles display.
        help_text="Tenant this row belongs to.",
    )
    feature_tier_overrides = models.JSONField(
        default=dict,
        help_text="JSON: {disabled_features: [...], tier_overrides: {...}}",
    )
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        managed = False
        db_table = "tenant_settings"
        verbose_name = "Tenant Setting"
        verbose_name_plural = "Tenant Settings"
        ordering = ["tenant_id"]

    def __str__(self) -> str:
        overrides = self.feature_tier_overrides or {}
        n_disabled = len(overrides.get("disabled_features", []))
        n_tier = len(overrides.get("tier_overrides", {}))
        return f"TenantSetting({self.tenant_id}) disabled={n_disabled} tier_overrides={n_tier}"
