/**
 * S5.9 — BFF proxy for password-reset request.
 *
 * Forwards POST /api/auth/reset/request → Django POST /api/auth/reset/request.
 * No cookies involved; the upstream response is just an acknowledgement.
 * Django always returns 200 regardless of whether the email matches a known
 * operator so callers cannot enumerate the user base.
 */

import { NextRequest, NextResponse } from "next/server";

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
    upstream = await fetch(`${DJANGO_AUTH_URL}/api/auth/reset/request`, {
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
  return NextResponse.json(data, { status: upstream.status });
}
