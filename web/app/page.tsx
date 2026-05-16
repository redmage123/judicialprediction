import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import Link from "next/link";
import { Button } from "@/components/ui/button";

// Authenticated operators land on the case list. Unauthenticated visitors get
// the marketing-style landing page below. The /healthz check is out of band
// (docker compose ps + scripts/jp-smoke) — it does not belong on a user-facing
// surface.
export default async function HomePage() {
  const cookieStore = await cookies();
  if (cookieStore.has("jp_session")) {
    redirect("/cases");
  }

  return (
    <main>
      {/* ── Top bar (unauthenticated) ─────────────────────────────────────── */}
      <header className="border-b">
        <div className="container mx-auto flex items-center justify-between px-6 py-4">
          <span className="text-lg font-semibold tracking-tight">JudicialPredict</span>
          <Button asChild variant="outline">
            <Link href="/login">Sign in</Link>
          </Button>
        </div>
      </header>

      {/* ── Hero ──────────────────────────────────────────────────────────── */}
      <section className="container mx-auto px-6 pt-20 pb-12 text-center">
        <p className="mx-auto mb-4 inline-block rounded-full border border-slate-200 bg-slate-50 px-3 py-1 text-xs font-medium uppercase tracking-wide text-slate-600">
          Case evaluation for litigation teams
        </p>
        <h1 className="mx-auto max-w-3xl text-4xl font-bold tracking-tight sm:text-5xl">
          Should you settle, try, or fold? Decide with data, not gut feel.
        </h1>
        <p className="mx-auto mt-6 max-w-2xl text-lg text-muted-foreground">
          JudicialPredict gives law firms an explainable P(win) probability,
          a conformal confidence interval, and a settle-versus-try expected-value
          comparison for every case — in seconds, with a signed audit trail.
        </p>
        <div className="mx-auto mt-10 flex max-w-xs flex-col items-stretch gap-3 sm:max-w-none sm:flex-row sm:items-center sm:justify-center">
          <Button asChild size="lg">
            <Link href="/login">Sign in to your workspace</Link>
          </Button>
          <Button asChild size="lg" variant="outline">
            <Link href="#how-it-works">See how it works</Link>
          </Button>
        </div>
      </section>

      {/* Scroll cue — animated chevron pointing to "How it works" so users
          know there's more below the hero on tall screens. */}
      <div className="flex justify-center pb-8">
        <Link
          href="#how-it-works"
          aria-label="Scroll to how it works"
          className="group inline-flex flex-col items-center gap-1 text-xs text-slate-700 hover:text-slate-900"
        >
          <span>Learn more</span>
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
            className="animate-bounce"
          >
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </Link>
      </div>

      {/* ── Value props ───────────────────────────────────────────────────── */}
      <section className="border-t bg-slate-50/50">
        <div className="container mx-auto grid gap-6 px-6 py-16 md:grid-cols-3">
          <article className="rounded-lg border bg-card p-6">
            <h2 className="text-lg font-semibold">Calibrated probability</h2>
            <p className="mt-2 text-sm text-muted-foreground">
              A gradient-boosted ensemble returns P(win) with a 90% conformal
              confidence interval. Wide intervals surface as visible
              low-confidence flags — no false precision.
            </p>
          </article>
          <article className="rounded-lg border bg-card p-6">
            <h2 className="text-lg font-semibold">Decision support, not a black box</h2>
            <p className="mt-2 text-sm text-muted-foreground">
              Every recommendation comes with the expected-value math behind it:
              EV(trial) vs EV(settlement), litigation cost, and a settle anchor —
              transparent reasoning the partner can defend.
            </p>
          </article>
          <article className="rounded-lg border bg-card p-6">
            <h2 className="text-lg font-semibold">Audit by default</h2>
            <p className="mt-2 text-sm text-muted-foreground">
              Every prediction is hashed, timestamped, signed by model version,
              and written to an immutable audit log. Download a PDF memo for the
              file at any time.
            </p>
          </article>
        </div>
      </section>

      {/* ── How it works ──────────────────────────────────────────────────── */}
      <section id="how-it-works" className="border-t">
        <div className="container mx-auto px-6 py-16">
          <div className="mx-auto max-w-2xl text-center">
            <h2 className="text-3xl font-bold tracking-tight">How it works</h2>
            <p className="mt-3 text-muted-foreground">
              Four steps from intake to a defensible recommendation.
            </p>
          </div>
          <ol className="mx-auto mt-12 grid max-w-4xl gap-6 sm:grid-cols-2 lg:grid-cols-4">
            {[
              {
                step: "01",
                title: "Submit case features",
                body: "Enter Tier-A/B inputs: judge severity, attorney win rate, jurisdiction, materiality. Tier-C party features are never accepted.",
              },
              {
                step: "02",
                title: "Get a prediction",
                body: "P(win) returns in under a second, with a 90% conformal CI and the exact model version that produced it.",
              },
              {
                step: "03",
                title: "Read the recommendation",
                body: "Settle, try, or borderline — with EV(trial) vs EV(settlement), the loss-exposure threshold, and the reasoning bullets.",
              },
              {
                step: "04",
                title: "Download the memo",
                body: "Generate a signed PDF for the case file. Re-run the prediction any time against the current champion model.",
              },
            ].map(({ step, title, body }) => (
              <li key={step} className="rounded-lg border bg-card p-6">
                <span className="text-xs font-mono text-muted-foreground">{step}</span>
                <h3 className="mt-2 text-base font-semibold">{title}</h3>
                <p className="mt-2 text-sm text-muted-foreground">{body}</p>
              </li>
            ))}
          </ol>
        </div>
      </section>

      {/* ── Final CTA ─────────────────────────────────────────────────────── */}
      <section className="border-t bg-slate-900 text-white">
        <div className="container mx-auto px-6 py-16 text-center">
          <h2 className="text-3xl font-bold tracking-tight">
            Ready to take the guesswork out of the settle-or-try call?
          </h2>
          <p className="mx-auto mt-3 max-w-xl text-sm text-slate-300">
            Sign in to your workspace and run your first prediction in under a
            minute. Your firm&apos;s data stays in your tenant — strict Row-Level
            Security on every query.
          </p>
          <div className="mt-8">
            <Button asChild size="lg" variant="outline" className="border-white text-white hover:bg-white hover:text-slate-900">
              <Link href="/login">Sign in</Link>
            </Button>
          </div>
        </div>
      </section>

      {/* ── Footer ────────────────────────────────────────────────────────── */}
      <footer className="border-t bg-slate-50/50">
        <div className="container mx-auto flex flex-col gap-4 px-6 py-8 sm:flex-row sm:items-center sm:justify-between">
          <p className="text-sm text-muted-foreground">
            © {new Date().getFullYear()} JudicialPredict
          </p>
          <nav className="flex flex-wrap items-center gap-x-5 gap-y-2 text-sm" aria-label="Footer">
            <Link href="/privacy" className="text-muted-foreground hover:text-foreground">
              Privacy
            </Link>
            <Link href="/cookies" className="text-muted-foreground hover:text-foreground">
              Cookies
            </Link>
            <Link href="/login" className="text-muted-foreground hover:text-foreground">
              Sign in
            </Link>
          </nav>
        </div>
      </footer>
    </main>
  );
}
