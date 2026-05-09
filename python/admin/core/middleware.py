"""
RLS-aware request middleware for the JudicialPredict admin console.

Sets the Postgres session variable ``app.current_tenant_id`` so that
Row-Level Security policies can enforce per-tenant data isolation.

Architecture note (ADR-003):
    The Postgres RLS policies in the baseline schema use:
        USING (id = current_setting('app.current_tenant_id', true)::uuid)
    This middleware provides the seam between the Django request context
    and the Postgres RLS layer.

Current scaffold limitations (Sprint-3 TODOs):
    1. The dev operator is hard-coded to a single tenant UUID.
       Real operators must be scoped via RBAC (see Sprint-3 epic).
    2. The database connection currently uses a Postgres superuser which
       bypasses RLS entirely — ``set_config`` is a no-op for superusers.
       Sprint-3 will switch to a ``jp_admin`` role with BYPASSRLS,
       where this middleware controls per-query row visibility.
    3. ``set_config`` uses ``is_local=false`` (session-scope) here.
       Sprint-3 should switch to ``is_local=true`` (transaction-scope)
       once ``ATOMIC_REQUESTS = True`` is confirmed stable for the admin.
"""

from django.db import connection

# Dev tenant UUID seeded by migration 20260507120001.
_DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001"

# Banner text injected into every admin response page (Sprint-3 removes this).
DEV_BANNER_MSG = (
    "[DEV] RLS bypass active: connected as Postgres superuser. "
    "Sprint-3 TODO: switch to jp_admin role with BYPASSRLS + per-operator scoping."
)


class RLSMiddleware:
    """
    Set ``app.current_tenant_id`` on the Postgres connection for every request.

    Runs as a ``process_view`` hook so it executes after authentication
    middleware has populated ``request.user``.  Unauthenticated requests
    (login page, static files) are skipped.

    .. note::
        The current dev default connects as a Postgres superuser, so the
        ``set_config`` call has no visible effect on query results — the
        superuser bypasses all RLS policies.  The middleware is the correct
        seam for Sprint-3 to wire up real operator scoping.
    """

    def __init__(self, get_response):
        self.get_response = get_response

    def __call__(self, request):
        return self.get_response(request)

    def process_view(self, request, view_func, view_args, view_kwargs):
        tenant_id = self._resolve_tenant(request)
        if tenant_id is not None:
            with connection.cursor() as cursor:
                # is_local=false → session-scope (survives connection pool reuse).
                # Sprint-3: change to is_local=true with ATOMIC_REQUESTS=True.
                cursor.execute(
                    "SELECT set_config('app.current_tenant_id', %s, false)",
                    [tenant_id],
                )
        return None  # continue normal request processing

    @staticmethod
    def _resolve_tenant(request) -> str | None:
        """
        Return the tenant UUID to apply for this request.

        Sprint-3 implementation: look up ``request.user``'s operator record
        in the RBAC table and return their scoped tenant UUID (or a sentinel
        that maps to BYPASSRLS for super-operators).
        """
        if not request.user.is_authenticated:
            return None
        # Scaffold: all authenticated operators see the dev tenant.
        return _DEV_TENANT_ID
