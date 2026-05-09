import { NextRequest, NextResponse } from "next/server";
import { SignJWT } from "jose";

// ---------------------------------------------------------------------------
// Dev-only hard-coded operator.  Sprint 4+ will replace with real SSO.
// ---------------------------------------------------------------------------
const DEV_EMAIL = "dev@example.test";
const DEV_PASSWORD = "dev-pass";
const DEV_OPERATOR_ID = "00000000-0000-0000-0000-000000000002";
const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

const FAKE_DEV_SECRET = "dev-only-NOT-A-REAL-SECRET-1234567890abcdef";

function getJwtSecret(): Uint8Array {
  const raw = process.env.JWT_DEV_SECRET ?? FAKE_DEV_SECRET;
  if (!process.env.JWT_DEV_SECRET) {
    console.warn(
      "[judicialpredict/web] JWT_DEV_SECRET is not set — using insecure " +
        "placeholder secret. Set JWT_DEV_SECRET in .env.local for dev."
    );
  }
  return new TextEncoder().encode(raw);
}

export async function POST(request: NextRequest) {
  let body: { email?: string; password?: string };
  try {
    body = await request.json();
  } catch {
    return NextResponse.json(
      { ok: false, error: "invalid_request" },
      { status: 400 }
    );
  }

  const { email, password } = body;

  // Constant-time-ish comparison (good enough for a dev gate — not prod auth).
  if (email !== DEV_EMAIL || password !== DEV_PASSWORD) {
    return NextResponse.json(
      { ok: false, error: "invalid_credentials" },
      { status: 401 }
    );
  }

  const secret = getJwtSecret();
  const token = await new SignJWT({
    sub: DEV_OPERATOR_ID,
    tenant_id: DEV_TENANT_ID,
    email: DEV_EMAIL,
  })
    .setProtectedHeader({ alg: "HS256" })
    .setIssuer("judicialpredict-web")
    .setAudience("judicialpredict-api")
    .setIssuedAt()
    .setExpirationTime("8h")
    .sign(secret);

  const response = NextResponse.json({ ok: true }, { status: 200 });

  // httpOnly so JS cannot read it; SameSite=Lax matches same-origin forms.
  // Secure should be true in production — Next.js sets it automatically when
  // NODE_ENV=production or the host is HTTPS.
  response.cookies.set("jp_session", token, {
    httpOnly: true,
    sameSite: "lax",
    path: "/",
    // 8 hours in seconds
    maxAge: 60 * 60 * 8,
  });

  return response;
}
