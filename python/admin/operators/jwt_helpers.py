"""
Shared JP-session JWT helpers — S6.6.

Both the password-login view (S4.8) and the OIDC SSO callback (S6.6) must
mint *identical* JP session tokens: same claims, same secret, same cookie.
This module is the single source of truth so the two auth paths cannot
drift.

JWT claims (must match api-gateway expectations — see rust/api-gateway/src/auth.rs):
    sub        UUID of the Operator record
    tenant_id  UUID of the operator's tenant (null for super operators)
    role       "admin" | "viewer" | "super"
    iss        "judicialpredict-admin"
    aud        "judicialpredict-api"
    iat / exp  issued-at / expiry (8-hour TTL)
"""

import datetime
import os

import jwt  # PyJWT

from operators.models import Operator

# The JWT_SECRET env var MUST match the value used by api-gateway.  A dev
# default is provided so the stack runs without configuration, but it MUST
# be replaced before any production deployment.
JWT_SECRET = os.environ.get(
    "JWT_SECRET",
    "dev-only-NOT-A-REAL-SECRET-1234567890abcdef",
)
JWT_TTL_HOURS = 8
COOKIE_NAME = "jp_session"
JWT_ISSUER = "judicialpredict-admin"
JWT_AUDIENCE = "judicialpredict-api"


def is_debug() -> bool:
    """Return True when running in DEBUG mode (dev/CI)."""
    return os.environ.get("DEBUG", "true").lower() not in ("false", "0", "no")


def mint_session_jwt(operator: Operator) -> str:
    """Sign an HS256 JP-session JWT for *operator*.

    Used by both the password-login view and the OIDC SSO callback so the
    two paths issue byte-for-byte equivalent tokens.
    """
    now = datetime.datetime.now(tz=datetime.timezone.utc)
    payload = {
        "sub": str(operator.id),
        "tenant_id": str(operator.tenant_id) if operator.tenant_id else None,
        "role": operator.role,
        "iss": JWT_ISSUER,
        "aud": JWT_AUDIENCE,
        "iat": int(now.timestamp()),
        "exp": int((now + datetime.timedelta(hours=JWT_TTL_HOURS)).timestamp()),
    }
    return jwt.encode(payload, JWT_SECRET, algorithm="HS256")


def set_session_cookie(response, token: str) -> None:
    """Attach the httpOnly jp_session cookie carrying *token* to *response*.

    Secure=True only outside DEBUG so the cookie works over plain HTTP in
    local dev.
    """
    response.set_cookie(
        COOKIE_NAME,
        token,
        httponly=True,
        samesite="Lax",
        secure=not is_debug(),
        max_age=JWT_TTL_HOURS * 3600,
        path="/",
    )
