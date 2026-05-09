"""
Database router for the JudicialPredict admin console.

Two-connection routing (S3.9)
------------------------------
``default``       jp_app DSN — subject to RLS, for tenant-scoped operators.
``admin_super``   jp_admin DSN — BYPASSRLS, for role='super' operators.

The ``RLSMiddleware`` sets ``_thread_local.db_alias`` on every request.
This router reads that value and returns the correct alias.

Special cases
-------------
- ``operators`` app models always use ``default`` regardless of the per-request
  alias.  This avoids a bootstrapping problem: the middleware must query the
  Operator table to *determine* the alias, so the Operator query must not itself
  be routed through the alias-under-determination.
- Django internal apps (auth, contenttypes, sessions, admin) also always use
  ``default`` — they live in the jp_app schema and are not tenant-scoped.
- ``allow_migrate`` restricts Django migrations to ``default`` only (never runs
  DDL via the admin_super alias).

Sprint-4 follow-ups
-------------------
- Add a read-replica ``replica`` alias for heavy analytics queries.
- Consider an ``audit`` alias for append-only audit_log writes.
"""

# Apps that must always use the default alias regardless of request context.
_ALWAYS_DEFAULT_APPS = frozenset(
    [
        "operators",          # bootstrapping: needed to resolve the alias
        "auth",               # Django auth
        "contenttypes",       # Django content types
        "sessions",           # Django sessions
        "admin",              # Django admin log
    ]
)


def _get_alias() -> str:
    """Read the per-request alias set by RLSMiddleware, with a safe fallback."""
    from core.middleware import get_current_db_alias  # local import avoids circular

    return get_current_db_alias()


class RLSRouter:
    """Route reads and writes based on the per-request operator role."""

    def db_for_read(self, model, **hints):
        if model._meta.app_label in _ALWAYS_DEFAULT_APPS:
            return "default"
        return _get_alias()

    def db_for_write(self, model, **hints):
        if model._meta.app_label in _ALWAYS_DEFAULT_APPS:
            return "default"
        return _get_alias()

    def allow_relation(self, obj1, obj2, **hints):
        # All models share the same physical database; relations are always valid.
        return True

    def allow_migrate(self, db, app_label, model_name=None, **hints):
        # Only apply Django migrations via the default connection.
        # The admin_super alias mirrors the same physical DB but we never run
        # DDL through it to avoid permission issues (jp_admin lacks DDL rights).
        return db == "default"
