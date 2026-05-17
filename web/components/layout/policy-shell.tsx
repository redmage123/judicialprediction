import Link from "next/link";
import { cookies } from "next/headers";
import { LogoutButton } from "@/components/layout/logout-button";
import { PrimaryNav } from "@/components/layout/nav";

/**
 * Thin app shell wrapper for marketing-style pages (Privacy, Cookies) that
 * shouldn't render bare. Audit finding UX-6 (2026-05-17): operators reach
 * these pages from the cookie banner and were previously stranded with no
 * way to navigate back. We render the same header the rest of the app
 * uses when authenticated, and a minimal anon-friendly header (just the
 * brand + Sign in) otherwise.
 *
 * Server component on purpose — we want the auth check to happen on the
 * server so there's no flash of either header during hydration.
 */
export async function PolicyShell({ children }: { children: React.ReactNode }) {
  const cookieStore = await cookies();
  const isAuthenticated = cookieStore.has("jp_session");

  return (
    <>
      {isAuthenticated ? (
        <header className="flex flex-wrap items-center justify-between gap-4 border-b px-6 py-3">
          <div className="flex items-center gap-6">
            <Link
              href="/cases"
              className="text-sm font-semibold tracking-tight hover:text-primary"
            >
              JudicialPredict
            </Link>
            <PrimaryNav />
          </div>
          <LogoutButton />
        </header>
      ) : (
        <header className="flex flex-wrap items-center justify-between gap-4 border-b px-6 py-3">
          <Link
            href="/"
            className="text-sm font-semibold tracking-tight hover:text-primary"
          >
            JudicialPredict
          </Link>
          <Link
            href="/login"
            className="text-sm text-muted-foreground hover:text-foreground"
          >
            Sign in
          </Link>
        </header>
      )}
      {children}
      <footer className="mt-12 border-t bg-slate-50/50">
        <div className="container mx-auto flex flex-col gap-4 px-6 py-8 sm:flex-row sm:items-center sm:justify-between">
          <p className="text-sm text-muted-foreground">
            © {new Date().getFullYear()} JudicialPredict
          </p>
          <nav
            className="flex flex-wrap items-center gap-x-5 gap-y-2 text-sm"
            aria-label="Footer"
          >
            <Link
              href="/privacy"
              className="text-muted-foreground hover:text-foreground"
            >
              Privacy
            </Link>
            <Link
              href="/cookies"
              className="text-muted-foreground hover:text-foreground"
            >
              Cookies
            </Link>
            {!isAuthenticated && (
              <Link
                href="/login"
                className="text-muted-foreground hover:text-foreground"
              >
                Sign in
              </Link>
            )}
          </nav>
        </div>
      </footer>
    </>
  );
}
