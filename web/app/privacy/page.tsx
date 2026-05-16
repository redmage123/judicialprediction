import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Privacy Policy — JudicialPredict",
};

// PLACEHOLDER COPY — replace with the final legal text before launch.
// Keep the structure (controller, lawful basis, retention, rights, contact)
// so the cookie banner's deep links remain valid.
export default function PrivacyPage() {
  return (
    <main className="mx-auto max-w-3xl px-6 py-12 text-sm leading-relaxed">
      <h1 className="mb-6 text-3xl font-bold tracking-tight">Privacy Policy</h1>
      <p className="text-xs text-muted-foreground">
        Last updated: 2026-05-16. <strong>This is a placeholder</strong> drafted
        for the development build. The production version will be reviewed by
        counsel before launch.
      </p>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Who we are</h2>
        <p>
          JudicialPredict is a legal-analytics platform that produces
          probability-of-success estimates for civil and criminal matters.
          Workspace operators access the service under a per-tenant agreement.
        </p>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">What we collect</h2>
        <ul className="list-disc space-y-1 pl-6">
          <li>Account data: email, name, role.</li>
          <li>
            Case feature inputs (Tier-A/B only): no party identifiers, no
            personal details about litigants.
          </li>
          <li>
            Operational telemetry: request timestamps, audit-log entries, error
            codes.
          </li>
        </ul>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Lawful basis</h2>
        <p>
          Processing is performed under a contract with the operator&apos;s
          organisation (GDPR Art. 6(1)(b)) and, where applicable, legitimate
          interest in maintaining service integrity (Art. 6(1)(f)).
        </p>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Retention</h2>
        <p>
          Predictions and audit-log entries are retained for the lifetime of
          the workspace plus 90 days. Operators may request earlier deletion
          via their workspace administrator.
        </p>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Your rights</h2>
        <p>
          You have the right to access, rectify, erase, restrict, and port your
          personal data, and to lodge a complaint with your supervisory
          authority. To exercise these rights, contact your workspace
          administrator or write to{" "}
          <a className="underline" href="mailto:privacy@judicialpredict.example">
            privacy@judicialpredict.example
          </a>
          .
        </p>
      </section>

      <section className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">Cookies</h2>
        <p>
          See the dedicated{" "}
          <a className="underline" href="/cookies">
            Cookie Policy
          </a>
          .
        </p>
      </section>
    </main>
  );
}
