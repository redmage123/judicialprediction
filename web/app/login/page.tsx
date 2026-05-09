import type { Metadata } from "next";
import { LoginForm } from "./login-form";

export const metadata: Metadata = {
  title: "Sign in — JudicialPredict",
};

/**
 * /login — server component.
 *
 * Reads ?next= from searchParams and passes a sanitised redirect target to
 * the client LoginForm island.  Only same-origin paths (starting with /) are
 * accepted; anything else falls back to "/".
 */
export default async function LoginPage({
  searchParams,
}: {
  searchParams: Promise<{ next?: string }>;
}) {
  const { next } = await searchParams;

  // Guard against open-redirect: only allow absolute paths, no protocols.
  const nextUrl =
    next && /^\/[^/]/.test(next) ? decodeURIComponent(next) : "/";

  return (
    <main className="flex min-h-screen items-center justify-center p-8">
      <LoginForm nextUrl={nextUrl} />
    </main>
  );
}
