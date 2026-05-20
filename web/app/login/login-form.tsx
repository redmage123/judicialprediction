"use client";

import Link from "next/link";
import { useState, type FormEvent } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { PasswordInput } from "@/components/ui/password-input";

interface LoginFormProps {
  /** Safe redirect path after successful login. Defaults to "/". */
  nextUrl: string;
  /** S6.6 — true when the Django auth service has OIDC SSO configured. */
  ssoEnabled?: boolean;
  /** S6.6 — human label for the SSO button (e.g. "Okta", "Entra ID"). */
  ssoProviderName?: string;
  /** S6.6 — failure code from a prior OIDC callback (?sso_error=...). */
  ssoError?: string | null;
}

/** S6.6 — map an OIDC callback failure code to a user-facing message. */
function ssoErrorMessage(code: string): string {
  switch (code) {
    case "unknown_operator":
      return "No JudicialPredict account is linked to that SSO identity. Contact your administrator.";
    case "no_email":
      return "Your identity provider did not share a verified email address.";
    case "exchange_failed":
      return "SSO sign-in could not be completed. Please try again.";
    default:
      return "SSO sign-in failed. Please try again or use your password.";
  }
}

export function LoginForm({
  nextUrl,
  ssoEnabled = false,
  ssoProviderName = "SSO",
  ssoError = null,
}: LoginFormProps) {
  const router = useRouter();
  const searchParams = useSearchParams();
  // After a successful password reset the operator is bounced back to /login
  // with ?reset=ok so we can show a "you can sign in now" confirmation.
  const justReset = searchParams.get("reset") === "ok";
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(
    ssoError ? ssoErrorMessage(ssoError) : null
  );
  const [pending, setPending] = useState(false);

  async function handleSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setError(null);
    setPending(true);

    try {
      const res = await fetch("/api/auth/login", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ email, password }),
      });

      if (res.ok) {
        // Cookie is set server-side; redirect to requested destination.
        router.push(nextUrl);
        router.refresh();
      } else {
        const data = await res.json().catch(() => ({}));
        setError(
          data.error === "invalid_credentials"
            ? "Invalid email/username or password."
            : "Sign-in failed. Please try again."
        );
      }
    } catch {
      setError("Network error. Please check your connection and try again.");
    } finally {
      setPending(false);
    }
  }

  return (
    <Card className="w-full max-w-sm">
      <CardHeader>
        <CardTitle className="text-2xl font-bold tracking-tight">
          JudicialPredict
        </CardTitle>
        <CardDescription>Sign in to your workspace</CardDescription>
      </CardHeader>
      <CardContent>
        {justReset && (
          <p
            role="status"
            aria-live="polite"
            className="mb-4 rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-800"
          >
            Password updated. Sign in with your new password.
          </p>
        )}
        <form onSubmit={handleSubmit} noValidate aria-label="Sign in">
          <div className="space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor="email">Email or username</Label>
              <Input
                id="email"
                name="email"
                type="text"
                autoComplete="username"
                required
                placeholder="you@example.com or username"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                aria-describedby={error ? "login-error" : "email-help"}
              />
              <p id="email-help" className="text-xs text-muted-foreground">
                Use the email associated with your operator account or your username.
              </p>
            </div>

            <div className="space-y-1.5">
              <div className="flex items-center justify-between">
                <Label htmlFor="password">Password</Label>
                <Link
                  href="/forgot-password"
                  className="text-xs font-medium text-muted-foreground underline underline-offset-2 hover:text-primary"
                >
                  Forgot password?
                </Link>
              </div>
              <PasswordInput
                id="password"
                name="password"
                autoComplete="current-password"
                required
                placeholder="••••••••"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                aria-describedby={error ? "login-error" : undefined}
              />
            </div>

            {error && (
              <p
                id="login-error"
                role="alert"
                aria-live="assertive"
                className="text-sm text-destructive"
              >
                {error}
              </p>
            )}

            <Button type="submit" size="lg" className="w-full" disabled={pending}>
              {pending ? "Signing in…" : "Sign in"}
            </Button>
          </div>
        </form>

        {/* S6.6 — SSO sign-in.  Only rendered when the Django auth service
            reports an OIDC IdP is configured.  This is a full-page
            navigation (not a fetch) because the OIDC flow is a chain of
            browser redirects: web → Django → IdP → Django → web. */}
        {ssoEnabled && (
          <>
            <div className="my-4 flex items-center gap-3" aria-hidden="true">
              <span className="h-px flex-1 bg-border" />
              <span className="text-xs text-muted-foreground">or</span>
              <span className="h-px flex-1 bg-border" />
            </div>
            <Button
              type="button"
              variant="outline"
              size="lg"
              className="w-full"
              onClick={() => {
                window.location.assign("/api/auth/sso/login");
              }}
            >
              Sign in with {ssoProviderName}
            </Button>
          </>
        )}
      </CardContent>
    </Card>
  );
}
