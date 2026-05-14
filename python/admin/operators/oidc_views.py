"""
OIDC SSO views — S6.6.

Three endpoints, all env-gated behind :func:`operators.oidc.oidc_enabled`:

GET /api/auth/sso/config
    Lightweight public probe.  Returns {enabled, provider_name} so the web
    login page can decide whether to render the "Sign in with SSO" button.
    Always 200 — when SSO is off it just returns {enabled: false}.

GET /api/auth/sso/login
    Initiates the OIDC authorization-code flow: redirects the browser to
    the IdP's authorize endpoint.  Authlib stashes state + nonce in the
    Django session.  404 when SSO is disabled.

GET /api/auth/sso/callback
    The IdP redirects back here with ?code&state.  We exchange the code,
    validate the id_token, resolve an existing active Operator by the
    verified email claim, mint the SAME JP-session JWT as password login,
    set the jp_session cookie, and 302 the browser to the web app.
    On any failure we 302 to <web>/login?sso_error=<reason> — never leak
    details to the browser.

The callback runs through the web app's BFF proxy so the browser only ever
sees one origin; see web/app/api/auth/sso/[...slug]/route.ts.
"""

import logging
from urllib.parse import urlencode

from django.conf import settings
from django.http import HttpResponseRedirect, JsonResponse
from django.views.decorators.csrf import csrf_exempt
from django.views.decorators.http import require_GET

from operators.jwt_helpers import mint_session_jwt, set_session_cookie
from operators.models import Operator
from operators.oidc import (
    email_from_userinfo,
    get_oidc_client,
    oidc_enabled,
    oidc_provider_name,
)

_log = logging.getLogger(__name__)


def _web_url(path: str, **params: str) -> str:
    """Build an absolute URL into the web app (for post-flow redirects)."""
    base = settings.WEB_BASE_URL.rstrip("/")
    qs = f"?{urlencode(params)}" if params else ""
    return f"{base}{path}{qs}"


def _callback_redirect_uri() -> str:
    """The callback URL registered with the IdP.

    Defaults to the web app's BFF proxy path so the browser stays
    same-origin; overridable via OIDC_REDIRECT_URI for non-standard
    deployments.
    """
    explicit = getattr(settings, "OIDC_REDIRECT_URI", "")
    if explicit:
        return explicit
    return _web_url("/api/auth/sso/callback")


@require_GET
def sso_config(request):
    """Public probe — is SSO available, and under what label?"""
    if not oidc_enabled():
        return JsonResponse({"enabled": False})
    return JsonResponse(
        {"enabled": True, "provider_name": oidc_provider_name()}
    )


@require_GET
def sso_login(request):
    """Kick off the OIDC authorization-code flow."""
    if not oidc_enabled():
        return JsonResponse({"ok": False, "error": "sso_disabled"}, status=404)

    client = get_oidc_client()
    # Authlib writes state + nonce into request.session; the callback reads
    # them back to defend against CSRF / replay.
    return client.authorize_redirect(request, _callback_redirect_uri())


@csrf_exempt
@require_GET
def sso_callback(request):
    """Handle the IdP redirect: exchange code, resolve operator, mint JWT."""
    if not oidc_enabled():
        return JsonResponse({"ok": False, "error": "sso_disabled"}, status=404)

    client = get_oidc_client()

    # 1. Exchange the authorization code for tokens.  Authlib validates the
    #    id_token signature, issuer, audience, nonce, and expiry.
    try:
        token = client.authorize_access_token(request)
    except Exception as exc:  # Authlib raises a variety of OAuth errors.
        _log.warning("OIDC token exchange failed: %s", exc)
        return HttpResponseRedirect(_web_url("/login", sso_error="exchange_failed"))

    # 2. Pull a verified email out of the token / userinfo.
    userinfo = None
    try:
        # Some IdPs put email only in the userinfo endpoint, not the id_token.
        if "userinfo" not in token:
            userinfo = client.userinfo(token=token)
    except Exception as exc:
        _log.warning("OIDC userinfo fetch failed: %s", exc)

    email = email_from_userinfo(token, userinfo)
    if not email:
        _log.warning("OIDC callback produced no verified email")
        return HttpResponseRedirect(_web_url("/login", sso_error="no_email"))

    # 3. Resolve an EXISTING active operator.  No auto-provisioning — an
    #    unknown SSO identity is a controlled failure (see oidc.py).
    try:
        operator = Operator.objects.get(email__iexact=email, is_active=True)
    except Operator.DoesNotExist:
        _log.warning("OIDC login for unprovisioned email=%s", email)
        return HttpResponseRedirect(_web_url("/login", sso_error="unknown_operator"))

    # 4. Mint the SAME JP-session JWT as password login and set the cookie.
    jp_token = mint_session_jwt(operator)
    response = HttpResponseRedirect(_web_url("/"))
    set_session_cookie(response, jp_token)
    _log.info("OIDC login succeeded operator=%s", operator.id)
    return response
