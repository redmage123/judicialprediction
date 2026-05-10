"""
Django admin registration for AuditLogEntry (S4.9 / JP-63).

Read-only view of the ``audit_log`` table.  No add, change, or delete
permissions are granted — this is an immutable audit trail.

RBAC / RLS routing
------------------
The RLSMiddleware (core/middleware.py) handles all connection routing:

  - role='admin' or role='viewer': request uses the ``default`` alias where
    ``app.current_tenant_id`` is set, so the Postgres RLS policy
    (audit_log_select) automatically filters rows to the operator's tenant.

  - role='super': request uses the ``admin_super`` alias (BYPASSRLS), so all
    rows across all tenants are visible.

We intentionally do NOT add a ``get_queryset`` filter here for tenant_id.
Adding a Python-side filter would double-filter for tenant-scoped operators
(redundant with RLS) and would incorrectly hide rows for super operators
if the logic were wrong.  RLS is the single source of truth.

Sprint-5 follow-ups
-------------------
- Export to CSV (download button in changelist toolbar).
- Row-level link from audit entry to source case/operator record.
- Full-text search on payload_hash (when that column is added to audit_log).
- Explicit date-range pickers (replace the built-in Today/This week shortcuts).
"""

from django.contrib import admin

from .models import AuditLogEntry


def _get_operator(request):
    """
    Return the active ``Operator`` for the current request, or ``None``.
    Always uses the ``default`` alias — mirrors the pattern in tenant_settings/admin.py.
    """
    from operators.models import Operator

    try:
        return Operator.objects.using("default").get(
            email=request.user.email, is_active=True
        )
    except Operator.DoesNotExist:
        return None


@admin.register(AuditLogEntry)
class AuditLogAdmin(admin.ModelAdmin):
    # ------------------------------------------------------------------
    # List view
    # ------------------------------------------------------------------

    list_display = ("ts", "action", "subject_id", "tenant_id", "reason_code", "latency_ms")
    list_filter = (
        "action",
        "reason_code",
        "table_name",
        ("ts", admin.DateFieldListFilter),
    )
    search_fields = ("subject_id", "action", "row_pk")
    ordering = ("-ts",)
    list_per_page = 50

    # ------------------------------------------------------------------
    # All fields read-only — this is an immutable audit trail.
    # ------------------------------------------------------------------

    readonly_fields = (
        "id",
        "tenant_id",
        "subject_id",
        "table_name",
        "row_pk",
        "action",
        "reason_code",
        "ts",
        "latency_ms",
        "cost_micros",
    )

    # ------------------------------------------------------------------
    # Permissions — read-only for all authenticated operators.
    # ------------------------------------------------------------------

    def has_module_perms(self, request, app_label=None):
        # Show the app in the admin index for any active operator.
        operator = _get_operator(request)
        return operator is not None

    def has_module_permission(self, request):
        operator = _get_operator(request)
        return operator is not None

    def has_view_permission(self, request, obj=None):
        operator = _get_operator(request)
        return operator is not None

    def has_add_permission(self, request):
        # Audit log is append-only via Rust; never allow Django admin inserts.
        return False

    def has_change_permission(self, request, obj=None):
        # Immutable — no edits.
        return False

    def has_delete_permission(self, request, obj=None):
        # Immutable — no deletes.
        return False
