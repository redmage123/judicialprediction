/**
 * GET /api/auth/me
 *
 * Reads the httpOnly jp_session cookie server-side, decodes the JWT claims
 * (no signature verification — that is the gateway's job), and returns them.
 * Returns { claims: null } when unauthenticated or token is expired.
 *
 * Used by the client-side AuthProvider to surface auth state without exposing
 * the raw JWT to browser JS.
 */
import { cookies } from "next/headers";
import { NextResponse } from "next/server";
import { decodeJwt } from "jose";

export async function GET() {
  const cookieStore = await cookies();
  const token = cookieStore.get("jp_session")?.value;

  if (!token) {
    return NextResponse.json({ claims: null });
  }

  try {
    const claims = decodeJwt(token);
    const now = Math.floor(Date.now() / 1000);
    if (claims.exp && claims.exp < now) {
      // Token expired — treat as unauthenticated.
      return NextResponse.json({ claims: null });
    }
    return NextResponse.json({ claims });
  } catch {
    return NextResponse.json({ claims: null });
  }
}
