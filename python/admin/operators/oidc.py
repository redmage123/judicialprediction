"""
OIDC client wiring for SSO sign-in — S6.6.

Uses Authlib's Django integration.  The whole feature is env-gated: when
``OIDC_ENABLED`` is false (the default), :func:`oidc_enabled` returns False,
the SSO endpoints 404, and the web login page hides the "Sign in with SSO"
button.  Nothing about the password-login path changes.

Configuration (all via env / Django settings)
----------------------------------------------
    OIDC_ENABLED          "true" to turn the feature on (default: false)
    OIDC_CLIENT_ID        OAuth2 client id registered with the IdP
    OIDC_CLIENT_SECRET    OAuth2 client secret
    OIDC_DISCOVERY_URL    IdP's OpenID well-known discovery document URL
                          (e.g. https://idp.example.com/.well-known/openid-configuration)
    OIDC_PROVIDER_NAME    Human label for the button (default: "SSO")

The redirect/callback URL registered with the IdP must point at the web
app's BFF proxy: ``<web-origin>/api/auth/sso/callback``.  The BFF forwards
it to Django's ``/api/auth/sso/callback`` so the browser only ever talks to
one origin and the jp_session cookie stays same-origin.

Operator resolution
-------------------
The callback resolves an *existing* active Operator by the IdP's verified
email claim.  We deliberately do NOT auto-provision operators here — that is
a tracked follow-up (see operators/models.py).  An unknown email is a
controlled failure, not a silent account creation.
"""

import logging

from authlib.integrations.django_client import OAuth
from django.conf import settings

_log = logging.getLogger(__name__)

# Authlib registry name for our single IdP.  Kept private — callers go
# through get_oidc_client() so the registration happens exactly once.
_OIDC_NAME = "jp_idp"

_oauth: OAuth | None = None


def oidc_enabled() -> bool:
    """True when SSO is configured and turned on."""
    return bool(
        getattr(settings, "OIDC_ENABLED", False)
        and getattr(settings, "OIDC_CLIENT_ID", "")
        and getattr(settings, "OIDC_CLIENT_SECRET", "")
        and getattr(settings, "OIDC_DISCOVERY_URL", "")
    )


def oidc_provider_name() -> str:
    """Human-readable IdP label for the login button."""
    return getattr(settings, "OIDC_PROVIDER_NAME", "SSO") or "SSO"


def get_oidc_client():
    """Return the lazily-registered Authlib OIDC client.

    Raises RuntimeError if OIDC is not enabled — callers must gate on
    :func:`oidc_enabled` first.
    """
    if not oidc_enabled():
        raise RuntimeError("OIDC is not enabled; check oidc_enabled() first")

    global _oauth
    if _oauth is None:
        oauth = OAuth()
        oauth.register(
            name=_OIDC_NAME,
            client_id=settings.OIDC_CLIENT_ID,
            client_secret=settings.OIDC_CLIENT_SECRET,
            server_metadata_url=settings.OIDC_DISCOVERY_URL,
            client_kwargs={"scope": "openid email profile"},
        )
        _oauth = oauth
        _log.info("OIDC client registered for provider=%s", oidc_provider_name())

    return getattr(_oauth, _OIDC_NAME)


def reset_oidc_client_for_tests() -> None:
    """Drop the cached client so tests can re-register with fresh settings."""
    global _oauth
    _oauth = None


def email_from_userinfo(token: dict, userinfo: dict | None) -> str | None:
    """Extract a verified email from the OIDC token/userinfo.

    Prefers the ``email`` claim in the parsed id_token; falls back to the
    userinfo endpoint response.  Returns None when no email is present or
    the IdP explicitly marks it unverified.
    """
    claims = (token or {}).get("userinfo") or userinfo or {}
    email = claims.get("email")
    if not email:
        return None
    # Respect an explicit `email_verified: false`.  When the claim is
    # absent we trust the IdP (many IdPs omit it for already-verified
    # corporate directories).
    if claims.get("email_verified") is False:
        _log.warning("OIDC email present but email_verified=false: %s", email)
        return None
    return email.strip().lower()
