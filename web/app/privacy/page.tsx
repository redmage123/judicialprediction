import type { Metadata } from "next";
import { PolicyShell } from "@/components/layout/policy-shell";

export const metadata: Metadata = {
  title: "Privacy Policy — JudicialPredict",
};

// COUNSEL REVIEW REQUIRED before production launch. This file covers the
// minimum a GDPR-style policy needs: controller identity, lawful basis,
// retention, rights, sub-processors, transfers, security, age, changes. It
// describes what the system actually does (Tier-A/B feature inputs only, no
// PII about parties, CourtListener as the data source for case law). When
// you replace this with the final policy keep the section IDs stable so the
// cookie banner's deep links don't break.
export default function PrivacyPage() {
  return (
    <PolicyShell>
    <main className="mx-auto max-w-3xl px-6 py-12 text-sm leading-relaxed">
      <h1 className="mb-2 text-3xl font-bold tracking-tight">Privacy Policy</h1>
      <p className="text-xs text-muted-foreground">
        Effective 2026-05-16. This document is the engineering team&apos;s
        draft; the production version will be reviewed and signed off by
        counsel before launch.
      </p>

      <section id="controller" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">1. Who we are</h2>
        <p>
          JudicialPredict (&quot;we&quot;, &quot;us&quot;) is a legal-analytics
          platform that produces calibrated probability-of-success estimates
          and recommended courses of action for civil and criminal matters.
          Workspace operators access the service under a per-organisation
          Master Services Agreement; for the purposes of GDPR / UK GDPR we
          act as a <strong>data processor</strong> for case-related data and
          as a <strong>data controller</strong> for operator account data.
        </p>
        <p>
          Contact for privacy questions:{" "}
          <a className="underline" href="mailto:privacy@judicialpredict.example">
            privacy@judicialpredict.example
          </a>
          . Postal address and DPO contact will appear here before launch.
        </p>
      </section>

      <section id="what-we-collect" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">2. What we collect</h2>
        <ul className="list-disc space-y-1 pl-6">
          <li>
            <strong>Account data</strong> &mdash; email, display name, role,
            tenant assignment, last sign-in timestamp. Provided by your
            workspace administrator at provisioning time.
          </li>
          <li>
            <strong>Case feature inputs</strong> &mdash; Tier-A and Tier-B
            features only (judge severity, attorney win rate, ideology
            distance, materiality score, procedural motion count, case type,
            jurisdiction). Tier-C party-identifying features are rejected at
            both the API gateway and the ML service.
          </li>
          <li>
            <strong>Operational telemetry</strong> &mdash; request
            timestamps, prediction outcomes, audit-log entries, error codes,
            latency metrics. Tied to the operator UUID; never to a party.
          </li>
          <li>
            <strong>Authentication cookie</strong> &mdash;{" "}
            <code className="rounded bg-muted px-1">jp_session</code>, a
            signed HS256 JWT in an HttpOnly cookie. See the{" "}
            <a className="underline" href="/cookies">Cookie Policy</a>.
          </li>
        </ul>
        <p>
          We do <strong>not</strong> collect: party names, race, gender,
          religion, immigration status, sealed-record information, or any
          other special-category data under GDPR Art. 9. The Tier-A/B
          allowlist is enforced in code (see{" "}
          <code className="rounded bg-muted px-1">predict.py:ALLOWLIST_FEATURES</code>
          {" "}and the equivalent guard in the Rust gateway).
        </p>
      </section>

      <section id="lawful-basis" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">3. Lawful basis</h2>
        <ul className="list-disc space-y-1 pl-6">
          <li>
            <strong>Contract</strong> (Art. 6(1)(b)) for the core service
            delivered to the operator&apos;s organisation.
          </li>
          <li>
            <strong>Legitimate interest</strong> (Art. 6(1)(f)) for
            operational telemetry, fraud and abuse prevention, and security
            monitoring. We balance this against operator rights and document
            the balancing test internally.
          </li>
          <li>
            <strong>Legal obligation</strong> (Art. 6(1)(c)) where we are
            required to retain records (e.g. tax, audit).
          </li>
        </ul>
      </section>

      <section id="sources" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">4. Sources of case-law data</h2>
        <p>
          Underlying case law is ingested from public sources, primarily{" "}
          <a className="underline" href="https://www.courtlistener.com/" rel="noreferrer">
            CourtListener
          </a>
          {" "}(Free Law Project, a US non-profit) under their terms of use.
          We store opinion text, citations, and derived structural features.
          We do not store party-identifying information beyond what is
          already in the public record.
        </p>
      </section>

      <section id="sub-processors" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">5. Sub-processors</h2>
        <p>
          We use the following sub-processors. The final list will appear
          here before launch:
        </p>
        <ul className="list-disc space-y-1 pl-6">
          <li>Cloud infrastructure provider (EU region) &mdash; compute and storage.</li>
          <li>Email delivery for transactional account messages.</li>
          <li>Error and performance monitoring.</li>
        </ul>
        <p>
          We do <strong>not</strong> use advertising networks, marketing
          analytics, or any third party that profiles operators or end
          users.
        </p>
      </section>

      <section id="transfers" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">6. International transfers</h2>
        <p>
          Case data is stored in EU data centres. Where transfers outside
          the EEA / UK occur (for example, to a US cloud control plane), we
          rely on the EU Standard Contractual Clauses (SCCs) and the
          UK&nbsp;IDTA, together with transfer impact assessments.
        </p>
      </section>

      <section id="retention" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">7. Retention</h2>
        <ul className="list-disc space-y-1 pl-6">
          <li>
            <strong>Predictions and audit-log entries:</strong> the lifetime
            of the workspace plus 90 days, then purged. Audit entries are
            immutable while retained.
          </li>
          <li>
            <strong>Account data:</strong> while the account is active, plus
            12 months for billing and audit purposes.
          </li>
          <li>
            <strong>Backups:</strong> rolling 30-day retention; deleted records
            age out of backups on the same schedule.
          </li>
        </ul>
        <p>
          Operators may request earlier deletion via their workspace
          administrator subject to legal-hold requirements.
        </p>
      </section>

      <section id="rights" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">8. Your rights</h2>
        <p>
          Under GDPR / UK GDPR you have the right to access, rectify, erase,
          restrict, and port your personal data, to object to processing
          based on legitimate interest, and to lodge a complaint with your
          national supervisory authority. To exercise these rights, contact
          your workspace administrator or write to{" "}
          <a className="underline" href="mailto:privacy@judicialpredict.example">
            privacy@judicialpredict.example
          </a>
          . We respond within 30 days.
        </p>
      </section>

      <section id="security" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">9. Security</h2>
        <ul className="list-disc space-y-1 pl-6">
          <li>
            Tenant data is isolated by Postgres row-level security (RLS) plus
            a per-request <code className="rounded bg-muted px-1">tenant_id</code>
            {" "}context; the database role used by the application has no
            <code className="rounded bg-muted px-1"> BYPASSRLS</code>.
          </li>
          <li>
            Operator authentication uses signed HS256 JWTs in HttpOnly,
            SameSite=Lax cookies. The <code className="rounded bg-muted px-1">Secure</code>
            {" "}flag is set in production.
          </li>
          <li>
            All cross-plane RPC traffic between the API gateway and the ML
            inference service runs over gRPC inside a private network.
          </li>
          <li>
            CSP, HSTS, X-Frame-Options=DENY, Referrer-Policy and
            Permissions-Policy headers are served on every response.
          </li>
        </ul>
      </section>

      <section id="children" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">10. Children</h2>
        <p>
          JudicialPredict is a B2B legal-analytics product not directed at
          children. We do not knowingly collect personal data from anyone
          under 16.
        </p>
      </section>

      <section id="cookies" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">11. Cookies</h2>
        <p>
          See the dedicated{" "}
          <a className="underline" href="/cookies">Cookie Policy</a>. In short:
          a single, necessary session cookie keeps you signed in. We do not
          run advertising or third-party tracking.
        </p>
      </section>

      <section id="changes" className="mt-8 space-y-3">
        <h2 className="text-lg font-semibold">12. Changes to this policy</h2>
        <p>
          Material changes will be notified by email and via an in-app
          banner at least 30 days before they take effect. The current
          version date is shown at the top of this page.
        </p>
      </section>
    </main>
    </PolicyShell>
  );
}
