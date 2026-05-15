import { NextRequest, NextResponse } from "next/server";
import { decodeJwt } from "jose";

/**
 * Route patterns that require an authenticated session.
 * Sprint 3: /case/:path*
 * Sprint 4 (S4.5): /cases added.
 * Sprint 6 (S6.12): /audit added - additionally gated on role admin.
 */
const PROTECTED_PREFIXES = ["/case", "/cases", "/audit"];

/**
 * S6.12 - routes that additionally require an admin JWT claim.
 *
 * The audit-log viewer (S6.12) extends S4.9 Django admin viewer into
 * the operator UI.  Tenant isolation is already enforced at the gateway
 * via Postgres RLS, but we still want to keep non-admin operators out of
 * the page entirely so the navigation can stay clean and so we have a
 * defense-in-depth boundary in front of the resolver.
 *
 * TODO(S6.12 follow-up): the existing JWT issuer (Django auth-proxy)
 * does NOT currently include a role claim.  Tokens minted today land
 * here with role === undefined and are redirected to
 * /login?denied=admin_required.  That is intentional: ship the gate
 * now and update the issuer next, rather than ship an open page that
 * has to be tightened later.  See python/admin/operators/models.py
 * for the Operator.role column we will mirror into the JWT.
 */
const ADMIN_ONLY_PREFIXES = ["/audit"];

/** Stable set of admin-equivalent role names (mirrors operators.Operator.role). */
const ADMIN_ROLES = new Set(["admin", "super"]);

function isProtectedPath(pathname: string): boolean {
  return PROTECTED_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(prefix + "/")
  );
}

function isAdminOnlyPath(pathname: string): boolean {
  return ADMIN_ONLY_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(prefix + "/")
  );
}

/** Build a /login redirect response, preserving the original path as ?next=. */
function redirectToLogin(
  request: NextRequest,
  opts: { clear?: boolean; denied?: string } = {}
): NextResponse {
  const url = request.nextUrl.clone();
  url.pathname = "/login";
  url.searchParams.set("next", request.nextUrl.pathname);
  if (opts.denied) {
    url.searchParams.set("denied", opts.denied);
  }

  const response = NextResponse.redirect(url, { status: 302 });
  if (opts.clear) {
    response.cookies.set("jp_session", "", {
      httpOnly: true,
      sameSite: "lax",
      path: "/",
      maxAge: 0,
    });
  }
  return response;
}

export function middleware(request: NextRequest): NextResponse {
  const { pathname } = request.nextUrl;

  if (!isProtectedPath(pathname)) {
    return NextResponse.next();
  }

  const token = request.cookies.get("jp_session")?.value;

  if (!token) {
    return redirectToLogin(request);
  }

  let claims: Record<string, unknown>;
  try {
    claims = decodeJwt(token) as Record<string, unknown>;
    const now = Math.floor(Date.now() / 1000);
    const exp = typeof claims.exp === "number" ? claims.exp : undefined;
    if (exp != null && exp < now) {
      // Expired - clear the stale cookie and redirect.
      return redirectToLogin(request, { clear: true });
    }
  } catch {
    // Malformed JWT - clear and redirect.
    return redirectToLogin(request, { clear: true });
  }

  // S6.12 - admin-only routes additionally require role admin (or super).
  // We do not clear the session cookie on a role-denied redirect: the operator
  // is still logged in, they just cannot view this surface.
  if (isAdminOnlyPath(pathname)) {
    const role = typeof claims.role === "string" ? claims.role : null;
    if (role == null || !ADMIN_ROLES.has(role)) {
      return redirectToLogin(request, { denied: "admin_required" });
    }
  }

  return NextResponse.next();
}

export const config = {
  matcher: ["/case/:path*", "/cases", "/cases/:path*", "/audit", "/audit/:path*"],
};
