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

import json
import logging
from urllib.parse import urlencode

from django.conf import settings
from django.contrib.auth import authenticate
from django.contrib.auth.password_validation import (
    ValidationError as PasswordValidationError,
)
from django.contrib.auth.password_validation import validate_password
from django.core.mail import send_mail
from django.http import HttpResponse, JsonResponse
from django.views.decorators.csrf import csrf_exempt
from django.views.decorators.http import require_POST

from operators.jwt_helpers import (
    COOKIE_NAME,
    mint_session_jwt,
    set_session_cookie,
)
from operators.models import (
    PASSWORD_RESET_TOKEN_TTL_MINUTES,
    Operator,
    PasswordResetToken,
)

_log = logging.getLogger(__name__)


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

    # S6.6: JWT minting + cookie are shared with the OIDC SSO callback so
    # the two auth paths issue identical tokens.  See operators/jwt_helpers.py.
    token = mint_session_jwt(operator)
    response = JsonResponse({"ok": True}, status=200)
    set_session_cookie(response, token)
    return response


@csrf_exempt
@require_POST
def logout(request):
    """Clear the jp_session cookie."""
    response = HttpResponse(status=204)
    response.delete_cookie(COOKIE_NAME, path="/", samesite="Lax")
    return response


# ---------------------------------------------------------------------------
# S5.9 — password reset (replaces the S4.8 "contact your admin" stub)
#
# POST /api/auth/reset/request
#     Body: {"email": "..."}
#     Always returns 200 with {ok: true} so callers cannot enumerate
#     registered emails.  If the email matches an active operator, a fresh
#     PasswordResetToken is created (1h TTL) and a reset link is emailed.
#
# POST /api/auth/reset/confirm
#     Body: {"token": "...", "new_password": "..."}
#     Validates the token (exists, not expired, not used), enforces Django's
#     password validators, hashes via Operator.set_password(), and marks the
#     token used.  On success returns 200 {ok: true}; on failure 400 with
#     {error: "invalid_token" | "weak_password"}.
# ---------------------------------------------------------------------------


@csrf_exempt
@require_POST
def password_reset_request(request):
    """Issue a 1h-TTL reset token + email the link (S5.9)."""
    try:
        body = json.loads(request.body)
    except (json.JSONDecodeError, ValueError):
        return JsonResponse({"ok": False, "error": "invalid_request"}, status=400)

    email = (body.get("email") or "").strip().lower()

    # Always return 200 to prevent email-enumeration attacks.  If the email
    # is unknown, we just don't send anything; the response shape is identical.
    response = JsonResponse(
        {
            "ok": True,
            "ttl_minutes": PASSWORD_RESET_TOKEN_TTL_MINUTES,
        },
        status=200,
    )

    if not email:
        return response

    try:
        operator = Operator.objects.get(email__iexact=email, is_active=True)
    except Operator.DoesNotExist:
        # Constant-time-ish: still cheap to skip the token creation here.
        _log.info("password reset requested for unknown email=%s", email)
        return response

    token = PasswordResetToken.issue_for(operator)
    reset_url = "{base}/reset-password?{qs}".format(
        base=settings.WEB_BASE_URL.rstrip("/"),
        qs=urlencode({"token": token.token}),
    )
    subject = "JudicialPredict — password reset request"
    body_text = (
        f"Hi,\n\n"
        f"We received a request to reset the password for the JudicialPredict "
        f"account associated with this email.\n\n"
        f"Set a new password by opening the link below within the next "
        f"{PASSWORD_RESET_TOKEN_TTL_MINUTES} minutes:\n\n"
        f"    {reset_url}\n\n"
        f"If you didn't request this, you can safely ignore this email.\n"
    )
    send_mail(
        subject=subject,
        message=body_text,
        from_email=settings.DEFAULT_FROM_EMAIL,
        recipient_list=[operator.email],
        fail_silently=False,
    )
    _log.info("password reset email sent operator=%s token_id=%s", operator.id, token.id)
    return response


@csrf_exempt
@require_POST
def password_reset_confirm(request):
    """Consume a reset token and set a new password (S5.9)."""
    try:
        body = json.loads(request.body)
    except (json.JSONDecodeError, ValueError):
        return JsonResponse({"ok": False, "error": "invalid_request"}, status=400)

    token_str = (body.get("token") or "").strip()
    new_password = body.get("new_password") or ""

    if not token_str or not new_password:
        return JsonResponse({"ok": False, "error": "invalid_request"}, status=400)

    try:
        token = PasswordResetToken.objects.select_related("operator").get(token=token_str)
    except PasswordResetToken.DoesNotExist:
        return JsonResponse({"ok": False, "error": "invalid_token"}, status=400)

    if not token.is_valid:
        return JsonResponse({"ok": False, "error": "invalid_token"}, status=400)

    operator = token.operator
    if not operator.is_active:
        return JsonResponse({"ok": False, "error": "invalid_token"}, status=400)

    try:
        validate_password(new_password, user=None)
    except PasswordValidationError as err:
        return JsonResponse(
            {"ok": False, "error": "weak_password", "details": list(err.messages)},
            status=400,
        )

    operator.set_password(new_password)
    operator.save(update_fields=["password", "updated_at"])
    token.mark_used()

    _log.info("password reset completed operator=%s token_id=%s", operator.id, token.id)
    return JsonResponse({"ok": True}, status=200)
