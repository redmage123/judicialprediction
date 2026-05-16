import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Cookie Policy — JudicialPredict",
};

// COUNSEL REVIEW REQUIRED before production launch. Keep the cookie table
// in sync with the cookies the BFF actually sets — at time of writing,
// jp_session is the only HTTP cookie. localStorage entries are described
// separately because cookie law treats them under the same regime even
// though they aren't strictly "cookies".
export default function CookiesPage() {
  return (
    <main className="mx-auto max-w-3xl px-6 py-12 text-sm leading-relaxed">
      <h1 className="mb-2 text-3xl font-bold tracking-tight">Cookie Policy</h1>
      <p className="text-xs text-muted-foreground">
        Effective 2026-05-16. This is the engineering team&apos;s draft and
        will be reviewed by counsel before launch.
      </p>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">What we use</h2>
        <p>
          JudicialPredict uses one HTTP cookie and one localStorage entry.
          We do not run advertising, marketing, or third-party analytics
          cookies. We do not embed third-party trackers, social plugins, or
          fingerprinting scripts.
        </p>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Cookies we set</h2>
        <div className="overflow-x-auto rounded-md border">
          <table className="w-full text-xs">
            <thead className="bg-muted/40">
              <tr className="text-left">
                <th className="px-3 py-2">Name</th>
                <th className="px-3 py-2">Purpose</th>
                <th className="px-3 py-2">Lifetime</th>
                <th className="px-3 py-2">Category</th>
              </tr>
            </thead>
            <tbody>
              <tr className="border-t">
                <td className="px-3 py-2 font-mono">jp_session</td>
                <td className="px-3 py-2">
                  Holds the signed JWT used to authenticate API requests
                  during a sign-in session. HttpOnly, SameSite=Lax, Secure in
                  production.
                </td>
                <td className="px-3 py-2">8 hours</td>
                <td className="px-3 py-2">Strictly necessary</td>
              </tr>
              <tr className="border-t">
                <td className="px-3 py-2 font-mono">csrftoken</td>
                <td className="px-3 py-2">
                  Django CSRF token used only on the Django admin console
                  pages at <code>/admin/</code>; not present on the main
                  workspace.
                </td>
                <td className="px-3 py-2">1 year</td>
                <td className="px-3 py-2">Strictly necessary</td>
              </tr>
            </tbody>
          </table>
        </div>
        <p className="text-xs text-muted-foreground">
          Under EU and UK cookie regulations, strictly necessary cookies
          required to deliver a service explicitly requested by the user do
          not require prior consent. We still surface them in the banner so
          you have full visibility.
        </p>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">localStorage</h2>
        <p>
          The application stores one localStorage entry to remember the
          choice you made in the cookie banner:
        </p>
        <div className="overflow-x-auto rounded-md border">
          <table className="w-full text-xs">
            <thead className="bg-muted/40">
              <tr className="text-left">
                <th className="px-3 py-2">Key</th>
                <th className="px-3 py-2">Value</th>
                <th className="px-3 py-2">Purpose</th>
              </tr>
            </thead>
            <tbody>
              <tr className="border-t">
                <td className="px-3 py-2 font-mono">jp.cookie-consent.v1</td>
                <td className="px-3 py-2 font-mono">accepted | rejected</td>
                <td className="px-3 py-2">
                  Records your banner choice so we don&apos;t show the banner
                  again on every page load. Never transmitted.
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Third-party cookies</h2>
        <p>
          None. JudicialPredict does not load third-party scripts that set
          cookies, and our Content-Security-Policy header restricts script,
          frame, and connection sources to our own origin.
        </p>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Changing your choice</h2>
        <p>
          Clear browser storage for this site (in most browsers:{" "}
          <em>Settings &rarr; Privacy &rarr; Clear site data</em>) to make
          the banner reappear, then make a fresh choice.
        </p>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Future changes</h2>
        <p>
          If we ever add analytics or other non-essential cookies, they will
          be gated behind explicit, granular consent and disclosed here
          before deployment.
        </p>
      </section>
    </main>
  );
}
