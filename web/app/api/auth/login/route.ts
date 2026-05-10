/**
 * S4.8 — BFF proxy for the Django auth service.
 *
 * Forwards POST /api/auth/login → Django POST /api/auth/login.
 * Django signs the HS256 JWT and sets the httpOnly jp_session cookie; this
 * proxy forwards the Set-Cookie header to the browser verbatim so the cookie
 * domain stays correct (same-origin from the browser's perspective).
 *
 * Sprint-5: replace with SAML/OIDC redirect when IdP is wired.
 */

import { NextRequest, NextResponse } from "next/server";

// Default: Django dev server on localhost.  Override via DJANGO_AUTH_URL in
// .env.local for docker-compose or staging environments.
const DJANGO_AUTH_URL = process.env.DJANGO_AUTH_URL ?? "http://localhost:8000";

export async function POST(request: NextRequest) {
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return NextResponse.json(
      { ok: false, error: "invalid_request" },
      { status: 400 }
    );
  }

  let upstream: Response;
  try {
    upstream = await fetch(`${DJANGO_AUTH_URL}/api/auth/login`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch {
    return NextResponse.json(
      { ok: false, error: "auth_service_unavailable" },
      { status: 502 }
    );
  }

  const data = await upstream.json().catch(() => ({}));
  const response = NextResponse.json(data, { status: upstream.status });

  // Forward the Set-Cookie from Django so the browser receives jp_session.
  const setCookie = upstream.headers.get("set-cookie");
  if (setCookie) {
    response.headers.set("set-cookie", setCookie);
  }

  return response;
}
