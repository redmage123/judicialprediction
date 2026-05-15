/**
 * Auth tests — S3.5 (updated S4.8)
 *
 * Covers:
 *  1. LoginForm renders required fields
 *  2. Happy path: login form POSTs to /api/auth/login (BFF proxy)
 *  3. Error path: 401 response shows inline error message
 *  4. Middleware: /case/new without cookie → 302 redirect to /login
 *  5. Middleware: /case/new with valid cookie → passes through
 *  6. a11y: login form passes axe-core gate
 *  7. BFF proxy route: forwards to DJANGO_AUTH_URL/api/auth/login (S4.8)
 *  8. Sprint-4 dev banner: renders the yellow info banner
 */

import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { axe, toHaveNoViolations } from "jest-axe";
import { NextRequest } from "next/server";

/**
 * Build a hand-rolled JWT string. The middleware uses jose.decodeJwt,
 * which only parses claims and does NOT verify the signature, so the
 * "signature" segment can be any base64url-safe placeholder. We avoid
 * jose.SignJWT here because its webapi build expects browser primitives
 * and fails under vitest's jsdom environment.
 */
function makeUnsignedJwt(claims: Record<string, unknown>): string {
  const b64url = (obj: object) =>
    Buffer.from(JSON.stringify(obj))
      .toString("base64")
      .replace(/=+$/, "")
      .replace(/\+/g, "-")
      .replace(/\//g, "_");
  const header = b64url({ alg: "HS256", typ: "JWT" });
  const payload = b64url(claims);
  return `${header}.${payload}.fake-signature-not-verified`;
}

// Extend matchers
expect.extend(toHaveNoViolations);

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

// vi.mock factories are hoisted ABOVE module-level const declarations, so
// referencing a top-level `vi.fn()` inside the factory yields `undefined`.
// vi.hoisted lets us declare the spies first and reference them safely.
const { mockRouterPush, mockRouterRefresh } = vi.hoisted(() => ({
  mockRouterPush: vi.fn(),
  mockRouterRefresh: vi.fn(),
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({
    push: mockRouterPush,
    refresh: mockRouterRefresh,
  }),
  // LoginForm reads ?reset=ok via useSearchParams; provide a minimal stub
  // with the .get() accessor the component uses.
  useSearchParams: () => new URLSearchParams(),
}));

// ---------------------------------------------------------------------------
// LoginForm tests
// ---------------------------------------------------------------------------

describe("LoginForm", () => {
  beforeEach(() => {
    // clearAllMocks only clears call history; resetAllMocks would also wipe
    // the mock implementations (including the next/navigation factory above),
    // which silently breaks router.push in the dynamically-imported component.
    vi.clearAllMocks();
  });

  async function renderForm() {
    // Dynamic import so the mock above is applied first.
    const { LoginForm } = await import("../app/login/login-form");
    return render(<LoginForm nextUrl="/" />);
  }

  it("renders email and password fields with a submit button", async () => {
    await renderForm();
    expect(screen.getByLabelText(/email/i)).toBeTruthy();
    expect(screen.getByLabelText(/password/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: /sign in/i })).toBeTruthy();
  });

  it("happy path: form POSTs to /api/auth/login (BFF proxy) on submit", async () => {
    // S4.8: the form still calls /api/auth/login — the BFF proxy forwards to
    // Django internally.  From the form's perspective the endpoint is unchanged.
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve({ ok: true }),
      })
    );

    await renderForm();

    fireEvent.change(screen.getByLabelText(/email/i), {
      target: { value: "dev-tenant1@example.test" },
    });
    fireEvent.change(screen.getByLabelText(/password/i), {
      target: { value: "tenant1-pw" },
    });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /sign in/i }));
    });

    await waitFor(
      () => {
        expect(vi.mocked(fetch)).toHaveBeenCalledWith(
          "/api/auth/login",
          expect.objectContaining({ method: "POST" })
        );
      },
      { timeout: 3000 }
    );

    // The fetch payload must contain the credentials.
    const fetchCall = vi.mocked(fetch).mock.calls[0];
    expect(fetchCall[1]?.body).toContain("dev-tenant1@example.test");
    expect(fetchCall[1]?.body).toContain("tenant1-pw");

    // Redirect-on-success is verified via the middleware test and E2E smoke.
  });

  it("error path: shows inline error on 401 invalid_credentials", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
        json: () => Promise.resolve({ ok: false, error: "invalid_credentials" }),
      })
    );

    await renderForm();

    fireEvent.change(screen.getByLabelText(/email/i), {
      target: { value: "wrong@example.test" },
    });
    fireEvent.change(screen.getByLabelText(/password/i), {
      target: { value: "wrong-pass" },
    });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /sign in/i }));
    });

    await waitFor(() => {
      expect(
        screen.getByRole("alert")
      ).toBeTruthy();
      expect(screen.getByText(/invalid email or password/i)).toBeTruthy();
    });

    // Router should NOT have been called.
    expect(mockRouterPush).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// S6.6 — OIDC SSO button + BFF proxy
// ---------------------------------------------------------------------------

describe("LoginForm — S6.6 SSO button", () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderForm(props: Record<string, unknown>) {
    const { LoginForm } = await import("../app/login/login-form");
    return render(<LoginForm nextUrl="/" {...props} />);
  }

  it("hides the SSO button when ssoEnabled is false (default)", async () => {
    await renderForm({});
    expect(screen.queryByRole("button", { name: /sign in with/i })).toBeNull();
  });

  it("shows a labelled SSO button when ssoEnabled is true", async () => {
    await renderForm({ ssoEnabled: true, ssoProviderName: "Example IdP" });
    expect(
      screen.getByRole("button", { name: /sign in with example idp/i })
    ).toBeTruthy();
  });

  it("clicking the SSO button navigates to the SSO login proxy", async () => {
    const assign = vi.fn();
    // jsdom's window.location.assign is a no-op stub; replace it with a spy.
    vi.stubGlobal("location", { ...window.location, assign });

    await renderForm({ ssoEnabled: true, ssoProviderName: "Okta" });
    fireEvent.click(screen.getByRole("button", { name: /sign in with okta/i }));

    expect(assign).toHaveBeenCalledWith("/api/auth/sso/login");
  });

  it("surfaces a prior OIDC callback error from the ssoError prop", async () => {
    await renderForm({ ssoError: "unknown_operator" });
    expect(screen.getByRole("alert")).toBeTruthy();
    expect(
      screen.getByText(/no judicialpredict account is linked/i)
    ).toBeTruthy();
  });
});

describe("S6.6 — SSO BFF proxy route", () => {
  beforeEach(() => vi.clearAllMocks());

  it("forwards GET /api/auth/sso/config to DJANGO_AUTH_URL", async () => {
    const mockUpstream = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ enabled: false }), {
        status: 200,
        headers: { "content-type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", mockUpstream);

    const mod = await import("../app/api/auth/sso/[...slug]/route");
    const { NextRequest } = await import("next/server");
    const req = new NextRequest(
      new Request("http://localhost:3000/api/auth/sso/config")
    );
    const res = await mod.GET(req, { params: Promise.resolve({ slug: ["config"] }) });

    expect(res.status).toBe(200);
    const upstreamUrl = mockUpstream.mock.calls[0][0] as string;
    expect(upstreamUrl).toMatch(/localhost:8000\/api\/auth\/sso\/config/);
  });

  it("rejects an unknown sub-path with 404 without calling upstream", async () => {
    const mockUpstream = vi.fn();
    vi.stubGlobal("fetch", mockUpstream);

    const mod = await import("../app/api/auth/sso/[...slug]/route");
    const { NextRequest } = await import("next/server");
    const req = new NextRequest(
      new Request("http://localhost:3000/api/auth/sso/evil")
    );
    const res = await mod.GET(req, { params: Promise.resolve({ slug: ["evil"] }) });

    expect(res.status).toBe(404);
    expect(mockUpstream).not.toHaveBeenCalled();
  });

  it("forwards a 302 redirect (login → IdP) back to the browser", async () => {
    const mockUpstream = vi.fn().mockResolvedValue(
      new Response(null, {
        status: 302,
        headers: { location: "https://idp.example.test/authorize?state=abc" },
      })
    );
    vi.stubGlobal("fetch", mockUpstream);

    const mod = await import("../app/api/auth/sso/[...slug]/route");
    const { NextRequest } = await import("next/server");
    const req = new NextRequest(
      new Request("http://localhost:3000/api/auth/sso/login")
    );
    const res = await mod.GET(req, { params: Promise.resolve({ slug: ["login"] }) });

    expect(res.status).toBe(302);
    expect(res.headers.get("location")).toBe(
      "https://idp.example.test/authorize?state=abc"
    );
    // The proxy must NOT auto-follow the redirect server-side.
    expect(mockUpstream).toHaveBeenCalledWith(
      expect.stringContaining("/api/auth/sso/login"),
      expect.objectContaining({ redirect: "manual" })
    );
  });
});

// ---------------------------------------------------------------------------
// Middleware tests
// ---------------------------------------------------------------------------

describe("middleware", () => {
  afterEach(() => vi.restoreAllMocks());

  it("redirects /case/new to /login when jp_session cookie is absent", async () => {
    const { middleware } = await import("../middleware");
    const req = new NextRequest("http://localhost:3000/case/new");
    const res = middleware(req);

    expect(res.status).toBe(302);
    const location = res.headers.get("location") ?? "";
    expect(location).toContain("/login");
    expect(location).toContain("next=%2Fcase%2Fnew");
  });

  it("passes through /case/new when a valid jp_session cookie is present", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = makeUnsignedJwt({
      sub: "00000000-0000-0000-0000-000000000002",
      tenant_id: "00000000-0000-0000-0000-000000000001",
      iat: now,
      exp: now + 3600, // 1h in the future
    });

    const { middleware } = await import("../middleware");

    const req = new NextRequest("http://localhost:3000/case/new", {
      headers: { cookie: `jp_session=${token}` },
    });
    const res = middleware(req);

    // NextResponse.next() returns 200 (no redirect).
    expect(res.status).toBe(200);
  });

  // S6.12 — operator-facing /audit is gated by middleware. Even without a JWT
  // role claim the unauthenticated case must redirect to /login first.
  it("redirects /audit to /login when jp_session cookie is absent", async () => {
    const { middleware } = await import("../middleware");
    const req = new NextRequest("http://localhost:3000/audit");
    const res = middleware(req);

    expect(res.status).toBe(302);
    const location = res.headers.get("location") ?? "";
    expect(location).toContain("/login");
    expect(location).toContain("next=%2Faudit");
  });
});

// ---------------------------------------------------------------------------
// a11y gate — /login page markup
// ---------------------------------------------------------------------------

describe("/login route — axe-core a11y gate", () => {
  it("login form passes axe-core with no violations", async () => {
    vi.mock("next/navigation", () => ({
      useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
      useSearchParams: () => new URLSearchParams(),
    }));

    const { LoginForm } = await import("../app/login/login-form");
    const { container } = render(<LoginForm nextUrl="/" />);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});

// ---------------------------------------------------------------------------
// S4.8 — BFF proxy route + Sprint-4 dev banner
// ---------------------------------------------------------------------------

describe("S4.8 — BFF proxy and dev banner", () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderForm() {
    const { LoginForm } = await import("../app/login/login-form");
    return render(<LoginForm nextUrl="/" />);
  }

  it("Sprint-4 dev banner is rendered in the login form", async () => {
    await renderForm();
    // The banner text is split across the requirement; match either half.
    expect(screen.getByText(/sprint 4.*real password auth/i)).toBeTruthy();
  });

  it("BFF proxy login route forwards to DJANGO_AUTH_URL/api/auth/login", async () => {
    // Import the route handler directly and mock global fetch.
    // We verify the upstream URL that the BFF calls — this confirms the proxy
    // wiring without requiring a live Django service.
    const mockUpstream = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "content-type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", mockUpstream);

    // Use dynamic import so process.env is read at call time.
    const mod = await import("../app/api/auth/login/route");
    const req = new Request("http://localhost:3000/api/auth/login", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ email: "dev-tenant1@example.test", password: "tenant1-pw" }),
    });

    // NextRequest wraps the standard Request.
    const { NextRequest } = await import("next/server");
    await mod.POST(new NextRequest(req));

    // The BFF must have called the Django endpoint.
    expect(mockUpstream).toHaveBeenCalledWith(
      expect.stringContaining("/api/auth/login"),
      expect.objectContaining({ method: "POST" })
    );
    const upstreamUrl: string = mockUpstream.mock.calls[0][0] as string;
    // Default DJANGO_AUTH_URL is http://localhost:8000 in test environment.
    expect(upstreamUrl).toMatch(/localhost:8000\/api\/auth\/login/);
  });
});
