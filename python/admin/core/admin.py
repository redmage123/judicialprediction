"""
Django admin registrations for JudicialPredict operator console.

Three model admins are registered (Tenant, Case, User).

get_queryset filtering (S3.9)
------------------------------
``TenantAdmin`` and ``CaseAdmin`` filter the queryset based on the
authenticated operator's role:

    role='super'          No filter — sees all rows (BYPASSRLS at the Postgres
                          level + unrestricted Django queryset).
    role='admin'/'viewer' Queryset filtered to the operator's ``tenant_id``
                          (belt-and-suspenders on top of Postgres RLS).

Permission gates
-----------------
- ``viewer`` operators get read-only access (has_add/change/delete → False).
- ``admin`` and ``super`` operators get full CRUD.

Sprint-4 follow-ups
-------------------
- Inline Cases inside TenantAdmin.
- LogEntry admin for audit trail.
- save_model / delete_model overrides to write to audit-recorder.
"""

from django.contrib import admin

from .models import Case, Tenant, User


# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------


def _get_operator(request):
    """
    Return the active ``Operator`` for this request, or ``None``.

    Uses ``using('default')`` directly — avoids routing through the
    thread-local alias (which may already be set to ``admin_super``).
    """
    from operators.models import Operator

    try:
        return Operator.objects.using("default").get(
            email=request.user.email, is_active=True
        )
    except Operator.DoesNotExist:
        return None


def _can_write(request) -> bool:
    operator = _get_operator(request)
    return operator is not None and operator.can_write


# ---------------------------------------------------------------------------
# Tenant
# ---------------------------------------------------------------------------


@admin.register(Tenant)
class TenantAdmin(admin.ModelAdmin):
    list_display = ("name", "slug", "created_at")
    search_fields = ("name", "slug")
    list_filter = ("created_at",)
    # id and created_at are immutable after creation.
    readonly_fields = ("id", "created_at")
    ordering = ("name",)

    def get_queryset(self, request):
        qs = super().get_queryset(request)
        operator = _get_operator(request)
        if operator is None or operator.is_super:
            return qs
        # Tenant-scoped: only the operator's own tenant row.
        if operator.tenant_id:
            return qs.filter(id=operator.tenant_id)
        return qs.none()

    def has_add_permission(self, request):
        return _can_write(request)

    def has_change_permission(self, request, obj=None):
        return _can_write(request)

    def has_delete_permission(self, request, obj=None):
        return _can_write(request)


# ---------------------------------------------------------------------------
# Case
# ---------------------------------------------------------------------------


@admin.register(Case)
class CaseAdmin(admin.ModelAdmin):
    list_display = ("title", "tenant", "jurisdiction", "judge_name", "created_at")
    search_fields = ("title", "judge_name", "court", "jurisdiction")
    list_filter = ("jurisdiction", "created_at")
    readonly_fields = ("id", "created_at", "updated_at")
    autocomplete_fields = ("tenant",)
    ordering = ("-created_at",)

    def get_queryset(self, request):
        qs = super().get_queryset(request)
        operator = _get_operator(request)
        if operator is None or operator.is_super:
            return qs
        if operator.tenant_id:
            return qs.filter(tenant_id=operator.tenant_id)
        return qs.none()

    def has_add_permission(self, request):
        return _can_write(request)

    def has_change_permission(self, request, obj=None):
        return _can_write(request)

    def has_delete_permission(self, request, obj=None):
        return _can_write(request)


# ---------------------------------------------------------------------------
# User
# ---------------------------------------------------------------------------


@admin.register(User)
class UserAdmin(admin.ModelAdmin):
    list_display = ("email", "tenant", "role", "created_at")
    search_fields = ("email",)
    list_filter = ("role", "created_at")
    readonly_fields = ("id", "created_at")
    autocomplete_fields = ("tenant",)
    ordering = ("email",)

    def has_add_permission(self, request):
        return _can_write(request)

    def has_change_permission(self, request, obj=None):
        return _can_write(request)

    def has_delete_permission(self, request, obj=None):
        return _can_write(request)
