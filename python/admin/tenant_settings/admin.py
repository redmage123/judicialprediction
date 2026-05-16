"""
Django admin registration for TenantSetting.

Access control (ADR-003)
------------------------
role='super'    Can view and edit any tenant's settings (all rows, BYPASSRLS).
role='admin'    Can view and edit their OWN tenant's settings only.
role='viewer'   Read-only; the change form renders in read-only mode.
No Operator     Django admin rejects the request at the RLSMiddleware level
                (403 before this code runs), but we guard defensively here too.

Sprint-3 dev banner
--------------------
SPRINT3_BANNER is injected as ``sprint3_banner`` into the change-form context.
The template (templates/admin/tenant_settings/tenantsetting/change_form.html)
renders it as a yellow warning box above the fieldsets.

Audit
-----
save_model() loads the pre-save overrides, calls super().save_model(), then
calls ``audit.record_override_changes()`` which writes one audit_log row per
changed override key.  Idempotent saves write zero rows.

Sprint-4 follow-up (JP-53): replace the direct-INSERT audit path (Option B)
with a gRPC call to UpdateTenantSettings (Option A) via
``core.feature_store_client.FeatureStoreClient``.
"""

from django.contrib import admin

from .audit import record_override_changes
from .forms import FEATURE_ORDER, TenantSettingForm
from .models import TenantSetting

# ---------------------------------------------------------------------------
# Sprint-3 banner text — persisted here as a module constant so it is easy
# to find and update without hunting through template files.
# ---------------------------------------------------------------------------

SPRINT3_BANNER: str = (
    "Override changes are audit-logged. "
    "Disabled features cannot be re-enabled by operators — "
    "only by a database superuser via direct migration."
)

# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------


def _get_operator(request):
    """
    Return the active ``Operator`` for the current request, or ``None``.
    Always queries the ``default`` alias regardless of the active routing
    alias — mirrors the pattern in core/admin.py.

    Django invokes ``has_module_permission`` for every registered ModelAdmin
    on the admin login page itself (to build the sidebar) — at which point
    ``request.user`` is ``AnonymousUser`` and has no ``email`` attribute.
    Guard against that so the login page renders instead of 500-ing.
    """
    from operators.models import Operator

    if not request.user.is_authenticated:
        return None

    try:
        return Operator.objects.using("default").get(
            email=request.user.email, is_active=True
        )
    except Operator.DoesNotExist:
        return None


# ---------------------------------------------------------------------------
# ModelAdmin
# ---------------------------------------------------------------------------


@admin.register(TenantSetting)
class TenantSettingAdmin(admin.ModelAdmin):
    form = TenantSettingForm

    # List view columns.
    list_display = ("tenant_id", "disabled_feature_count", "tier_override_count", "updated_at")
    ordering = ("tenant_id",)

    # tenant_id / timestamps are immutable from the admin side.
    readonly_fields = ("tenant_id", "created_at", "updated_at")

    fieldsets = [
        (
            "Tenant",
            {
                "fields": ["tenant_id", "created_at", "updated_at"],
            },
        ),
        (
            "Disabled Features",
            {
                "description": (
                    "Features checked here are refused with PERMISSION_DENIED "
                    "regardless of their global tier assignment. "
                    "Only tightening is permitted; disabled features cannot be "
                    "re-enabled from this UI."
                ),
                "fields": ["disabled_features"],
            },
        ),
        (
            "Tier Overrides",
            {
                "description": (
                    "Set to Tier B or Tier C to downgrade a feature for this tenant. "
                    "Only downgrading (A→B, A→C, B→C) is permitted. "
                    "Setting Tier C is equivalent to disabling the feature (PERMISSION_DENIED)."
                ),
                "fields": [f"tier_override_{f}" for f in FEATURE_ORDER],
            },
        ),
    ]

    # ------------------------------------------------------------------
    # List-view computed columns
    # ------------------------------------------------------------------

    @admin.display(description="Disabled features")
    def disabled_feature_count(self, obj: TenantSetting) -> int:
        overrides = obj.feature_tier_overrides or {}
        return len(overrides.get("disabled_features", []))

    @admin.display(description="Tier overrides")
    def tier_override_count(self, obj: TenantSetting) -> int:
        overrides = obj.feature_tier_overrides or {}
        return len(overrides.get("tier_overrides", {}))

    # ------------------------------------------------------------------
    # RBAC gates
    # ------------------------------------------------------------------

    def get_queryset(self, request):
        qs = super().get_queryset(request)
        operator = _get_operator(request)
        if operator is None:
            return qs.none()
        if operator.is_super:
            # Super-operators see all tenants (BYPASSRLS at DB level).
            return qs
        if operator.tenant_id:
            # Tenant-scoped operators see only their own row.
            return qs.filter(tenant_id=operator.tenant_id)
        return qs.none()

    def has_module_perms(self, request):
        return _get_operator(request) is not None

    def has_view_permission(self, request, obj=None):
        return _get_operator(request) is not None

    def has_change_permission(self, request, obj=None):
        operator = _get_operator(request)
        return operator is not None and operator.can_write

    def has_add_permission(self, request):
        operator = _get_operator(request)
        return operator is not None and operator.can_write

    def has_delete_permission(self, request, obj=None):
        # Immutable audit-trail principle: no deletes from the admin.
        return False

    # ------------------------------------------------------------------
    # Banner injection
    # ------------------------------------------------------------------

    def _inject_banner(self, extra_context: dict | None) -> dict:
        ctx = extra_context or {}
        ctx["sprint3_banner"] = SPRINT3_BANNER
        return ctx

    def change_view(self, request, object_id, form_url="", extra_context=None):
        return super().change_view(
            request, object_id, form_url, self._inject_banner(extra_context)
        )

    def add_view(self, request, form_url="", extra_context=None):
        return super().add_view(
            request, form_url, self._inject_banner(extra_context)
        )

    # ------------------------------------------------------------------
    # Save + audit
    # ------------------------------------------------------------------

    def save_model(self, request, obj: TenantSetting, form, change: bool):
        new_overrides: dict = form.cleaned_data.get("_packed_overrides", {})

        # Load the pre-save state for diff computation.
        old_overrides: dict = {}
        if change:
            try:
                old_obj = TenantSetting.objects.get(pk=obj.pk)
                old_overrides = old_obj.feature_tier_overrides or {}
            except TenantSetting.DoesNotExist:
                pass

        # Apply the packed overrides to the instance before persisting.
        obj.feature_tier_overrides = new_overrides
        super().save_model(request, obj, form, change)

        # Write audit rows (0 rows if no diff — idempotent).
        record_override_changes(
            tenant_id=obj.tenant_id,
            old_overrides=old_overrides,
            new_overrides=new_overrides,
            actor_email=request.user.email,
        )
