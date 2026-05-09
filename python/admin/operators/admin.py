"""
Django admin registration for the Operator model.

Visible only to Django superusers (is_superuser=True).  Normal tenant-scoped
operators never see this view — their admin access is scoped by get_queryset()
in the core app's model admins.

Sprint-4 follow-ups
-------------------
- Self-service onboarding form: operator requests access, admin approves.
- Restrict add/change/delete to a ``jp_ops`` permission group.
- Log operator provisioning events to audit_log.
"""

from django.contrib import admin

from .models import Operator


@admin.register(Operator)
class OperatorAdmin(admin.ModelAdmin):
    list_display = ("email", "role", "tenant_id", "is_active", "created_at")
    list_filter = ("role", "is_active")
    search_fields = ("email",)
    readonly_fields = ("id", "created_at", "updated_at")
    ordering = ("email",)

    def get_queryset(self, request):
        # Only Django superusers may list operators.
        qs = super().get_queryset(request)
        if not request.user.is_superuser:
            return qs.none()
        return qs

    def has_module_perms(self, request):
        return request.user.is_superuser

    def has_view_permission(self, request, obj=None):
        return request.user.is_superuser

    def has_change_permission(self, request, obj=None):
        return request.user.is_superuser

    def has_add_permission(self, request):
        return request.user.is_superuser

    def has_delete_permission(self, request, obj=None):
        return request.user.is_superuser
