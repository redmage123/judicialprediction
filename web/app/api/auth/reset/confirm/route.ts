/**
 * S5.9 — BFF proxy for password-reset confirm.
 *
 * Forwards POST /api/auth/reset/confirm → Django POST /api/auth/reset/confirm.
 * Django validates the token + new password and either rotates the operator's
 * bcrypt hash (200 ok) or returns 400 with `invalid_token` / `weak_password`.
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
    upstream = await fetch(`${DJANGO_AUTH_URL}/api/auth/reset/confirm`, {
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
