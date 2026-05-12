"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { PasswordInput } from "@/components/ui/password-input";

/**
 * /reset-password — confirm step.  Submits {token, new_password}; on success,
 * sends the operator back to /login with a success flag (the new password
 * isn't auto-logged-in — they have to type it once to prove they remember it).
 */
export function ResetPasswordForm({ token }: { token: string }) {
  const router = useRouter();
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [details, setDetails] = useState<string[]>([]);
  const [pending, setPending] = useState(false);

  async function handleSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setError(null);
    setDetails([]);

    if (!token) {
      setError("This reset link is missing its token. Request a new one.");
      return;
    }
    if (password !== confirm) {
      setError("Passwords don't match.");
      return;
    }

    setPending(true);
    try {
      const res = await fetch("/api/auth/reset/confirm", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ token, new_password: password }),
      });
      const data = await res.json().catch(() => ({}));

      if (res.ok) {
        router.push("/login?reset=ok");
      } else if (data.error === "invalid_token") {
        setError(
          "This reset link is no longer valid. Request a new one and try again."
        );
      } else if (data.error === "weak_password") {
        setError("Please choose a stronger password.");
        if (Array.isArray(data.details)) {
          setDetails(data.details as string[]);
        }
      } else {
        setError("Something went wrong. Please try again in a moment.");
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
          Set a new password
        </CardTitle>
        <CardDescription>
          Choose a password you haven&apos;t used before. The reset link can only
          be used once.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} noValidate aria-label="Set new password">
          <div className="space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor="password">New password</Label>
              <PasswordInput
                id="password"
                name="password"
                autoComplete="new-password"
                required
                minLength={8}
                placeholder="••••••••"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                aria-describedby={error ? "reset-error" : undefined}
              />
            </div>

            <div className="space-y-1.5">
              <Label htmlFor="confirm">Confirm password</Label>
              <PasswordInput
                id="confirm"
                name="confirm"
                autoComplete="new-password"
                required
                minLength={8}
                placeholder="••••••••"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
                aria-describedby={error ? "reset-error" : undefined}
              />
            </div>

            {error && (
              <div id="reset-error" role="alert" aria-live="assertive">
                <p className="text-sm text-destructive">{error}</p>
                {details.length > 0 && (
                  <ul className="mt-2 list-disc pl-5 text-sm text-destructive">
                    {details.map((d, i) => (
                      <li key={i}>{d}</li>
                    ))}
                  </ul>
                )}
              </div>
            )}

            <Button type="submit" size="lg" className="w-full" disabled={pending}>
              {pending ? "Updating…" : "Update password"}
            </Button>
          </div>
        </form>
        <p className="mt-6 text-sm text-muted-foreground">
          Need a fresh link?{" "}
          <Link
            href="/forgot-password"
            className="font-medium underline underline-offset-2 hover:text-primary"
          >
            Request a new one
          </Link>
        </p>
      </CardContent>
    </Card>
  );
}
