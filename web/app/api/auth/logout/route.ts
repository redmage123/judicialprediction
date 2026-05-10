/**
 * S4.8 — BFF proxy for Django logout.
 *
 * Forwards POST /api/auth/logout → Django POST /api/auth/logout.
 * Django clears the jp_session cookie via Set-Cookie; this proxy forwards
 * that header.  Falls back to clearing the cookie locally if Django is
 * unreachable so the browser session is always cleaned up.
 */

import { NextResponse } from "next/server";

const DJANGO_AUTH_URL = process.env.DJANGO_AUTH_URL ?? "http://localhost:8000";

export async function POST() {
  let upstream: Response | null = null;
  try {
    upstream = await fetch(`${DJANGO_AUTH_URL}/api/auth/logout`, {
      method: "POST",
      headers: { "content-type": "application/json" },
    });
  } catch {
    // Django unreachable — clear the cookie locally and return 204.
  }

  const response = new NextResponse(null, { status: 204 });

  if (upstream) {
    const setCookie = upstream.headers.get("set-cookie");
    if (setCookie) {
      response.headers.set("set-cookie", setCookie);
      return response;
    }
  }

  // Fallback: clear the cookie in the BFF layer.
  response.cookies.set("jp_session", "", {
    httpOnly: true,
    sameSite: "lax",
    path: "/",
    maxAge: 0,
  });
  return response;
}
