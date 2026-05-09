"""
Database router for the JudicialPredict admin console.

Routes all reads and writes to the ``default`` database.  The unmanaged
application models (Tenant, Case, User) must never be migrated by Django —
their DDL is managed entirely by the Rust migration system.

Sprint-3 TODOs:
    - Add a read-replica database alias (``replica``) and route reads there.
    - Consider a separate ``audit`` alias for the audit_log table to allow
      independent scaling of the append-only audit path.
"""


class RLSRouter:
    """Route all database operations to the ``default`` alias."""

    def db_for_read(self, model, **hints):
        return "default"

    def db_for_write(self, model, **hints):
        return "default"

    def allow_relation(self, obj1, obj2, **hints):
        # Allow relations between any two models — all live in the same DB.
        return True

    def allow_migrate(self, db, app_label, model_name=None, **hints):
        # Only Django's own managed apps may migrate (auth, sessions, etc.).
        # Application models (managed=False) never produce migrations.
        return db == "default"
