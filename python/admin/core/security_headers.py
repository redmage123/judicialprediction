"""
Tiny response-header hygiene middleware (PEN audit, OWASP A05).

The default Django dev server (``wsgiref.simple_server``) sets a
``Server: WSGIServer/0.2 CPython/3.12.12`` banner that leaks framework and
runtime version.  Overwriting it with an empty string in middleware causes
``wsgiref`` to suppress its own value because app-supplied headers win.

In production we sit behind nginx where ``server_tokens off`` does the
same job, but the dev box gets audited too — strip it here.

Also asserts a baseline set of security headers (CSP, HSTS, XFO, etc.)
which Django's ``SecurityMiddleware`` only partially covers.
"""
from __future__ import annotations

from typing import Callable

# Conservative dev CSP — the admin only loads its own static assets.
# Production should serve a stricter policy via the reverse proxy.
_CSP = (
    "default-src 'self'; "
    "script-src 'self' 'unsafe-inline'; "
    "style-src 'self' 'unsafe-inline'; "
    "img-src 'self' data:; "
    "font-src 'self' data:; "
    "connect-src 'self'; "
    "frame-ancestors 'none'; "
    "form-action 'self'; "
    "base-uri 'self'; "
    "object-src 'none'"
)

_HEADERS = {
    "Server": "",  # blank wins over wsgiref's default banner
    "Content-Security-Policy": _CSP,
    "Strict-Transport-Security": "max-age=5184000; includeSubDomains",
    "X-Content-Type-Options": "nosniff",
    "X-Frame-Options": "DENY",
    "Referrer-Policy": "strict-origin-when-cross-origin",
    "Permissions-Policy": (
        "accelerometer=(), camera=(), geolocation=(), gyroscope=(), "
        "magnetometer=(), microphone=(), payment=(), usb=()"
    ),
}


class SecurityHeadersMiddleware:
    """Apply baseline security headers + strip server-version banner."""

    def __init__(self, get_response: Callable):
        self.get_response = get_response

    def __call__(self, request):
        response = self.get_response(request)
        for name, value in _HEADERS.items():
            response[name] = value
        return response
