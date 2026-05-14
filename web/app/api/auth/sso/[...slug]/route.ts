/**
 * S6.6 — BFF proxy for the OIDC SSO endpoints.
 *
 * Catch-all GET proxy for:
 *   /api/auth/sso/config    → Django /api/auth/sso/config    (JSON probe)
 *   /api/auth/sso/login     → Django /api/auth/sso/login     (302 → IdP)
 *   /api/auth/sso/callback  → Django /api/auth/sso/callback  (302 → web + cookie)
 *
 * Why a proxy at all
 * ------------------
 * Routing the whole OIDC dance through the web origin keeps every cookie
 * same-origin: the browser only ever talks to the Next.js app, never to
 * Django directly.  That matters for two cookies:
 *   - Django's `sessionid` — Authlib stashes the OIDC state/nonce there on
 *     /login and reads it back on /callback, so it must round-trip.
 *   - `jp_session` — minted by Django on a successful callback; the browser
 *     must receive it on the web origin for the rest of the app to see it.
 *
 * So this proxy forwards cookies in BOTH directions and does NOT auto-follow
 * redirects (`redirect: "manual"`) — it hands the 3xx + Location straight
 * back to the browser.
 */

import { NextRequest, NextResponse } from "next/server";

const DJANGO_AUTH_URL = process.env.DJANGO_AUTH_URL ?? "http://localhost:8000";

// Only these sub-paths are proxied; anything else is a 404.  Keeps the
// catch-all from forwarding arbitrary segments to Django.
const ALLOWED_SLUGS = new Set(["config", "login", "callback"]);

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ slug: string[] }> }
) {
  const { slug } = await params;
  const sub = slug.join("/");
  if (!ALLOWED_SLUGS.has(sub)) {
    return NextResponse.json({ ok: false, error: "not_found" }, { status: 404 });
  }

  // Preserve the query string (?code&state on the callback).
  const search = request.nextUrl.search;
  const upstreamUrl = `${DJANGO_AUTH_URL}/api/auth/sso/${sub}${search}`;

  let upstream: Response;
  try {
    upstream = await fetch(upstreamUrl, {
      method: "GET",
      // Forward the browser's cookies so Django sees its own sessionid
      // (carries the Authlib OIDC state/nonce between login and callback).
      headers: {
        cookie: request.headers.get("cookie") ?? "",
      },
      // Hand 3xx responses back to the browser instead of chasing them
      // server-side — the redirect targets (IdP, web app) are browser-bound.
      redirect: "manual",
    });
  } catch {
    return NextResponse.json(
      { ok: false, error: "auth_service_unavailable" },
      { status: 502 }
    );
  }

  // Build the response, preserving status (302/200/404) and Location.
  const location = upstream.headers.get("location");
  let response: NextResponse;
  if (location) {
    // A redirect (login → IdP, callback → web app).  NextResponse.redirect
    // requires an absolute URL; Django always emits absolute Locations here.
    response = NextResponse.redirect(location, { status: upstream.status });
  } else {
    const body = await upstream.text();
    response = new NextResponse(body, {
      status: upstream.status,
      headers: {
        "content-type":
          upstream.headers.get("content-type") ?? "application/json",
      },
    });
  }

  // Forward every Set-Cookie verbatim so both sessionid (login) and
  // jp_session (callback) land on the web origin.
  for (const cookie of upstream.headers.getSetCookie()) {
    response.headers.append("set-cookie", cookie);
  }

  return response;
}
