"""
AuditLogEntry — unmanaged Django model over the Rust-owned ``audit_log`` table.

Schema source of truth:
    rust/feature-store/migrations/20260507120004_audit_log_rls_and_outbound_cols.sql

This model is read-only from the admin console perspective.  Django never
creates, alters, or drops the underlying table (managed=False).

RLS semantics
-------------
Tenant-scoped operators (role=admin/viewer) connect via the ``default`` alias
where ``app.current_tenant_id`` is set by RLSMiddleware.  The Postgres RLS
policy on audit_log (audit_log_select) scopes SELECT to rows whose tenant_id
matches that setting.

Super-operators connect via ``admin_super`` (BYPASSRLS) and see all rows.

Neither this model nor the admin.py needs any Python-side queryset filtering
for RBAC — the RLS policy enforces it at the DB level.  See audit_log/admin.py
for the explanatory comment.
"""

from django.db import models


class AuditLogEntry(models.Model):
    """Read-only projection of the ``audit_log`` Postgres table."""

    id = models.BigIntegerField(primary_key=True)
    tenant_id = models.UUIDField(null=True, blank=True)
    # The operator/actor that triggered the event (e.g. operator UUID or email).
    subject_id = models.TextField(null=True, blank=True)
    table_name = models.TextField()
    # Primary key of the affected row in table_name (nullable for non-row events).
    row_pk = models.TextField(null=True, blank=True)
    action = models.TextField()
    reason_code = models.TextField(null=True, blank=True)
    # Column is named `ts` in Postgres (not created_at / updated_at).
    ts = models.DateTimeField(db_column="ts")
    latency_ms = models.IntegerField(null=True, blank=True)
    cost_micros = models.IntegerField(null=True, blank=True)

    class Meta:
        managed = False
        db_table = "audit_log"
        ordering = ["-ts"]
        verbose_name = "Audit log entry"
        verbose_name_plural = "Audit log entries"

    def __str__(self) -> str:
        return f"[{self.ts}] {self.action} tenant={self.tenant_id}"
