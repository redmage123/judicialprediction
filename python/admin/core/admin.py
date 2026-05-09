"""
Django admin registrations for JudicialPredict operator console.

Three model admins are registered (Tenant, Case, User).  All are read-only
by default — writes are audited in Sprint-3 when the audit-log viewer and
operator RBAC are in place.

Sprint-3 TODOs in this file:
    - Limit queryset per-operator once RBAC is wired.
    - Add inline for Cases inside TenantAdmin.
    - Add LogEntry admin for audit trail.
    - Enable save_model / delete_model overrides to fire audit events.
"""

from django.contrib import admin

from .models import Case, Tenant, User


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
