import { NextRequest, NextResponse } from "next/server";
import { decodeJwt } from "jose";

/**
 * Route patterns that require an authenticated session.
 * Sprint 3: /case/:path*
 * Sprint 4 (S4.5): /cases added.
 */
const PROTECTED_PREFIXES = ["/case", "/cases"];

function isProtectedPath(pathname: string): boolean {
  return PROTECTED_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(prefix + "/")
  );
}

/** Build a /login redirect response, preserving the original path as ?next=. */
function redirectToLogin(request: NextRequest, clear = false): NextResponse {
  const url = request.nextUrl.clone();
  url.pathname = "/login";
  url.searchParams.set("next", request.nextUrl.pathname);

  const response = NextResponse.redirect(url, { status: 302 });
  if (clear) {
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

  try {
    const claims = decodeJwt(token);
    const now = Math.floor(Date.now() / 1000);
    if (claims.exp && claims.exp < now) {
      // Expired — clear the stale cookie and redirect.
      return redirectToLogin(request, true);
    }
  } catch {
    // Malformed JWT — clear and redirect.
    return redirectToLogin(request, true);
  }

  return NextResponse.next();
}

export const config = {
  matcher: ["/case/:path*", "/cases", "/cases/:path*"],
};
