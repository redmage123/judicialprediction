import type { Metadata } from "next";
import { LoginForm } from "./login-form";

export const metadata: Metadata = {
  title: "Sign in — JudicialPredict",
};

// Server-side env: the Django auth service origin.  Same default as the
// BFF proxy routes.
const DJANGO_AUTH_URL = process.env.DJANGO_AUTH_URL ?? "http://localhost:8000";

/** S6.6 — shape of the /api/auth/sso/config probe handed to the form. */
interface SsoConfig {
  enabled: boolean;
  providerName: string;
}

/**
 * Probe the Django auth service for OIDC availability.  Done server-side so
 * the "Sign in with SSO" button renders without a client-side flicker.
 * Any failure (auth service down, OIDC misconfigured) degrades gracefully
 * to password-only login.
 */
async function loadSsoConfig(): Promise<SsoConfig> {
  try {
    const res = await fetch(`${DJANGO_AUTH_URL}/api/auth/sso/config`, {
      cache: "no-store",
    });
    if (!res.ok) return { enabled: false, providerName: "SSO" };
    const data = await res.json();
    return {
      enabled: data.enabled === true,
      providerName: data.provider_name ?? "SSO",
    };
  } catch {
    return { enabled: false, providerName: "SSO" };
  }
}

/**
 * /login — server component.
 *
 * Reads ?next= from searchParams and passes a sanitised redirect target to
 * the client LoginForm island.  Only same-origin paths (starting with /) are
 * accepted; anything else falls back to "/".
 *
 * S6.6: also reads ?sso_error= (set by the OIDC callback on failure) and
 * probes the auth service for SSO availability.
 */
export default async function LoginPage({
  searchParams,
}: {
  searchParams: Promise<{ next?: string; sso_error?: string }>;
}) {
  const { next, sso_error: ssoError } = await searchParams;

  // Guard against open-redirect: only allow absolute paths, no protocols.
  const nextUrl =
    next && /^\/[^/]/.test(next) ? decodeURIComponent(next) : "/";

  const sso = await loadSsoConfig();

  return (
    <main className="flex min-h-screen items-center justify-center p-8">
      <LoginForm
        nextUrl={nextUrl}
        ssoEnabled={sso.enabled}
        ssoProviderName={sso.providerName}
        ssoError={ssoError ?? null}
      />
    </main>
  );
}
