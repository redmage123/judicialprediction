"""
RLS-aware request middleware for the JudicialPredict admin console.

Replaces the S2.15 scaffold (hard-coded dev tenant + Postgres superuser).

How it works (ADR-003)
----------------------
1.  Every authenticated request triggers ``process_view``.
2.  The middleware looks up the ``Operator`` record whose email matches
    ``request.user.email``.
3.  Depending on ``operator.role``:

    role='admin' or role='viewer'
        - Uses the ``default`` DATABASES alias (jp_app, subject to RLS).
        - Issues ``SET CONFIG app.current_tenant_id`` so the RLS policies in
          Postgres scope all subsequent queries on this connection to
          ``operator.tenant_id``.

    role='super'
        - Uses the ``admin_super`` DATABASES alias (jp_admin, BYPASSRLS).
        - Does NOT set ``app.current_tenant_id``; jp_admin bypasses RLS and
          sees all rows.

4.  If no Operator record exists for the authenticated user, returns HTTP 403
    with a clear message.

Thread-local state
------------------
``_thread_local.db_alias`` is set before ``get_response`` is called so that
the ``RLSRouter`` database router can pick the correct alias for every query
in the request/response cycle.  It is reset at the end of ``__call__`` via a
``finally`` block.

Connection scope
----------------
``set_config(key, value, is_local=True)`` is transaction-scoped.  This requires
``ATOMIC_REQUESTS = True`` (set in settings.py) so the transaction wraps the
whole request — safe with psycopg3 + Django 5 on Postgres.
"""

import threading

from django.db import connections
from django.http import HttpResponseForbidden

# Thread-local stores the chosen DB alias for the duration of this request.
_thread_local = threading.local()

# Sentinel used before process_view resolves the operator.
_DEFAULT_ALIAS = "default"


def get_current_db_alias() -> str:
    """Return the DB alias set by ``RLSMiddleware`` for the current thread."""
    return getattr(_thread_local, "db_alias", _DEFAULT_ALIAS)


class RLSMiddleware:
    """
    Per-request RBAC resolution and Postgres RLS scope setter.

    Must run AFTER ``django.contrib.auth.middleware.AuthenticationMiddleware``
    so that ``request.user`` is populated.
    """

    def __init__(self, get_response):
        self.get_response = get_response

    def __call__(self, request):
        # Ensure a clean slate for every request (thread pool reuse safety).
        _thread_local.db_alias = _DEFAULT_ALIAS
        try:
            response = self.get_response(request)
        finally:
            _thread_local.db_alias = _DEFAULT_ALIAS
        return response

    def process_view(self, request, view_func, view_args, view_kwargs):  # noqa: ARG002
        if not request.user.is_authenticated:
            return None  # login page / static files — skip

        operator = _resolve_operator(request.user.email)
        if operator is None:
            return HttpResponseForbidden(
                "No operator profile found for this account. "
                "Ask an admin to provision one via the Operators panel."
            )

        if not operator.is_active:
            return HttpResponseForbidden(
                "Your operator account has been deactivated. Contact an admin."
            )

        if operator.is_super:
            # Super operators route to jp_admin (BYPASSRLS); no set_config needed.
            _thread_local.db_alias = "admin_super"
        else:
            # Tenant-scoped operators use jp_app; set RLS session variable.
            _thread_local.db_alias = "default"
            if operator.tenant_id is not None:
                with connections["default"].cursor() as cursor:
                    # is_local=True → transaction-scoped (requires ATOMIC_REQUESTS=True).
                    cursor.execute(
                        "SELECT set_config('app.current_tenant_id', %s, true)",
                        [str(operator.tenant_id)],
                    )

        return None  # continue normal request processing


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _resolve_operator(email: str):
    """
    Return the active ``Operator`` for *email*, or ``None`` if not found.

    Always queries the ``default`` alias — the Operator table is always
    accessible regardless of the current request's routing alias.
    """
    from operators.models import Operator  # local import avoids circular refs

    try:
        return Operator.objects.using("default").get(email=email, is_active=True)
    except Operator.DoesNotExist:
        return None
