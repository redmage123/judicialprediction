"""
Auth views for the JudicialPredict Django admin service — S4.8.

POST /api/auth/login
    Accepts {email, password} JSON.
    Authenticates via OperatorAuthBackend (bcrypt check against Operator.password).
    On success: signs an HS256 JWT and sets it as an httpOnly jp_session cookie.
    On failure: 401 {ok: false, error: "invalid_credentials"}.

POST /api/auth/logout
    Clears the jp_session cookie.  204 No Content.

JWT claims (must match api-gateway expectations — see rust/api-gateway/src/auth.rs):
    sub        UUID of the Operator record
    tenant_id  UUID of the operator's tenant (null for super operators)
    role       "admin" | "viewer" | "super"
    iss        "judicialpredict-admin"
    aud        "judicialpredict-api"
    iat / exp  issued-at / expiry (8-hour TTL)

Security notes
--------------
- The JWT_SECRET env var MUST match the value used by api-gateway ($JWT_SECRET).
  A dev default is provided so the stack runs without configuration, but it MUST
  be replaced before any production deployment.
- The Secure flag on jp_session is set when DEBUG=False.  In local dev over HTTP
  the flag is omitted so the browser accepts the cookie.
- csrf_exempt is safe here because the endpoint is JSON-only (no form POST) and
  the cookie is httpOnly + SameSite=Lax, which provides CSRF protection.

Sprint-5 follow-ups
-------------------
- SAML/OIDC: replace this view with a redirect to the IdP.
- Rotate JWT_SECRET independent of the Django SECRET_KEY rotation cycle.
- Add rate-limiting middleware around /api/auth/login.
"""

import datetime
import json
import os

import jwt  # PyJWT
from django.contrib.auth import authenticate
from django.http import HttpResponse, JsonResponse
from django.views.decorators.csrf import csrf_exempt
from django.views.decorators.http import require_POST

from operators.models import Operator

_JWT_SECRET = os.environ.get(
    "JWT_SECRET",
    "dev-only-NOT-A-REAL-SECRET-1234567890abcdef",
)
_JWT_TTL_HOURS = 8
_COOKIE_NAME = "jp_session"


@csrf_exempt
@require_POST
def login(request):
    """Authenticate operator and set jp_session cookie."""
    try:
        body = json.loads(request.body)
    except (json.JSONDecodeError, ValueError):
        return JsonResponse({"ok": False, "error": "invalid_request"}, status=400)

    email = body.get("email", "")
    password = body.get("password", "")

    # OperatorAuthBackend.authenticate() verifies the bcrypt hash.
    user = authenticate(request, username=email, password=password)
    if user is None:
        return JsonResponse({"ok": False, "error": "invalid_credentials"}, status=401)

    # Fetch the Operator row to build JWT claims.
    try:
        operator = Operator.objects.get(email=email, is_active=True)
    except Operator.DoesNotExist:
        return JsonResponse({"ok": False, "error": "invalid_credentials"}, status=401)

    now = datetime.datetime.now(tz=datetime.timezone.utc)
    payload = {
        "sub": str(operator.id),
        "tenant_id": str(operator.tenant_id) if operator.tenant_id else None,
        "role": operator.role,
        "iss": "judicialpredict-admin",
        "aud": "judicialpredict-api",
        "iat": int(now.timestamp()),
        "exp": int((now + datetime.timedelta(hours=_JWT_TTL_HOURS)).timestamp()),
    }
    token = jwt.encode(payload, _JWT_SECRET, algorithm="HS256")

    response = JsonResponse({"ok": True}, status=200)
    # Secure=True only outside DEBUG so the cookie works over plain HTTP in dev.
    use_secure = not _is_debug()
    response.set_cookie(
        _COOKIE_NAME,
        token,
        httponly=True,
        samesite="Lax",
        secure=use_secure,
        max_age=_JWT_TTL_HOURS * 3600,
        path="/",
    )
    return response


@csrf_exempt
@require_POST
def logout(request):
    """Clear the jp_session cookie."""
    response = HttpResponse(status=204)
    response.delete_cookie(_COOKIE_NAME, path="/", samesite="Lax")
    return response


def _is_debug() -> bool:
    """Return True when running in DEBUG mode (dev/CI)."""
    return os.environ.get("DEBUG", "true").lower() not in ("false", "0", "no")
