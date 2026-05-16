import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Cookie Policy — JudicialPredict",
};

// PLACEHOLDER COPY — replace before launch. Inventory below must match the
// cookies actually set by the BFF (currently: only `jp_session`).
export default function CookiesPage() {
  return (
    <main className="mx-auto max-w-3xl px-6 py-12 text-sm leading-relaxed">
      <h1 className="mb-6 text-3xl font-bold tracking-tight">Cookie Policy</h1>
      <p className="text-xs text-muted-foreground">
        Last updated: 2026-05-16. <strong>This is a placeholder</strong> drafted
        for the development build.
      </p>

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
                  Holds the signed JWT used to authenticate API requests.
                </td>
                <td className="px-3 py-2">Session</td>
                <td className="px-3 py-2">Strictly necessary</td>
              </tr>
            </tbody>
          </table>
        </div>
        <p className="text-xs text-muted-foreground">
          We do not currently set analytics, marketing, or third-party tracking
          cookies. If that changes, the cookie banner will gate the new cookies
          behind explicit consent.
        </p>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">localStorage</h2>
        <p>
          The application stores one localStorage entry —{" "}
          <code className="rounded bg-muted px-1">jp.cookie-consent.v1</code>{" "}
          — to record your banner choice. It is never transmitted.
        </p>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Changing your choice</h2>
        <p>
          Clear browser storage for this site (
          <em>browser settings → privacy → clear site data</em>) to make the
          banner reappear.
        </p>
      </section>
    </main>
  );
}
