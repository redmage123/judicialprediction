// S4.4 (JP-58): converted from a stateful client island to a purely
// presentational component that receives server-fetched data as a prop.
//
// S4.7 (JP-61): promoted to a client component to host two inline client
// islands — RepredictButton and PredictionHistoryDisclosure — that require
// useMutation / useQuery / useState.  The top-level component (ResultsView)
// and its layout children remain stateless; only the two islands carry state.
//
// Design choice: keep all S4.7 UI in this single file (rather than splitting
// into repredict-button.tsx and prediction-history.tsx) to stay within the
// sprint scope constraint and minimise file proliferation.
//
// Sprint-5 follow-up: once the component grows beyond ~250 LOC consider
// extracting RepredictButton and PredictionHistoryDisclosure to their own
// files under web/app/case/[id]/.

"use client";

import { useState } from "react";
import Link from "next/link";
import { useMutation, useQuery } from "@apollo/client/react";
import { useRouter } from "next/navigation";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import type { CaseResult } from "@/lib/queries/predict";
import {
  REPREDICT_CASE,
  GET_CASE_PREDICTIONS,
  type RepredictCaseData,
  type RepredictCaseVars,
  type GetCasePredictionsData,
  type GetCasePredictionsVars,
} from "@/lib/queries/predict";

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

function fmtPercent(p: number): string {
  return `${Math.round(p * 100)}%`;
}

function fmtDollar(s: string): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 0,
  }).format(parseFloat(s));
}

const USD_INLINE = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 0,
});

// Server-generated rationale bullets embed raw floats like `$32319.71`.
// Normalise them to `$32,320` so they match the card values below.
function formatBulletAmounts(s: string): string {
  return s.replace(/\$\s?(\d+(?:\.\d+)?)/g, (_m, n: string) =>
    USD_INLINE.format(parseFloat(n))
  );
}

function badgeVariantForKind(
  kind: string
): "settle" | "try" | "warning" {
  if (kind === "Try") return "try";
  if (kind === "Settle") return "settle";
  return "warning";
}

// First 8 hex chars of the run_id are enough to identify a model version
// without exposing 32-char MLflow internal IDs to the operator.
function shortModelVersion(version: string): string {
  if (!version) return "";
  return version.length > 12 ? `${version.slice(0, 8)}` : version;
}

// ---------------------------------------------------------------------------
// Client island: RepredictButton
//
// Calls the repredictCase mutation with the current case id.  On completion,
// calls router.refresh() so the RSC parent re-fetches the page with the new
// prediction (Next.js App Router soft-refresh pattern).
//
// Sprint-5 follow-up: accept operator-supplied expected_damages so
// decision-arith can be re-run alongside the ML prediction.
// ---------------------------------------------------------------------------

interface RepredictButtonProps {
  caseId: string;
}

function RepredictButton({ caseId }: RepredictButtonProps) {
  const router = useRouter();
  const [repredictCase, { loading }] = useMutation<
    RepredictCaseData,
    RepredictCaseVars
  >(REPREDICT_CASE, {
    variables: { id: caseId },
    onCompleted: () => router.refresh(),
  });

  return (
    <button
      type="button"
      onClick={() => repredictCase({ variables: { id: caseId } })}
      disabled={loading}
      aria-label="Re-run with latest model"
      className="inline-flex h-11 items-center justify-center rounded-md border border-input bg-background px-4 text-sm font-medium shadow-sm hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50"
    >
      {loading ? "Running…" : "Re-run with latest model"}
    </button>
  );
}

// ---------------------------------------------------------------------------
// Client island: PredictionHistoryDisclosure
//
// A collapsible section showing the full prediction history for a case.
// The GET_CASE_PREDICTIONS query is skipped until the first open so the
// common case (operator viewing current result only) incurs zero extra
// GraphQL round-trips.  Subsequent toggles reuse the Apollo cache.
// ---------------------------------------------------------------------------

interface PredictionHistoryDisclosureProps {
  caseId: string;
}

function PredictionHistoryDisclosure({ caseId }: PredictionHistoryDisclosureProps) {
  const [open, setOpen] = useState(false);

  const { data, loading } = useQuery<
    GetCasePredictionsData,
    GetCasePredictionsVars
  >(GET_CASE_PREDICTIONS, {
    variables: { id: caseId },
    skip: !open,
  });

  const entries = data?.casePredictions ?? [];

  return (
    <section aria-label="Prediction history">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="text-sm font-medium text-primary underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
      >
        {open ? "Hide" : "Show"} prediction history
        {entries.length > 0 && ` (${entries.length} run${entries.length !== 1 ? "s" : ""})`}
      </button>

      {open && (
        <div className="mt-2 space-y-1" role="list" aria-label="Past prediction runs">
          {loading && (
            <p className="text-xs text-muted-foreground">Loading history…</p>
          )}
          {entries.map((entry) => (
            <div
              key={entry.id}
              role="listitem"
              className="flex gap-4 text-xs text-muted-foreground"
            >
              <span>{new Date(entry.createdAt).toLocaleDateString("en-US")}</span>
              <span>P(win): {Math.round(entry.prediction.pWin * 100)}%</span>
              <span className="font-mono">{entry.modelVersion}</span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

// ---------------------------------------------------------------------------
// Empty state — case not found or wrong tenant
// ---------------------------------------------------------------------------

function EmptyState() {
  return (
    <main
      className="flex min-h-screen items-center justify-center p-8"
      aria-label="Case results not found"
    >
      <div className="w-full max-w-md text-center space-y-4">
        <h1 className="text-2xl font-bold">Results not available</h1>
        <p className="text-sm text-muted-foreground">
          This case&apos;s prediction data has expired or could not be found.
          Please submit a new case to see results.
        </p>
        <Link
          href="/case/new"
          className="inline-flex items-center justify-center rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
        >
          Submit a new case
        </Link>
      </div>
    </main>
  );
}

// ---------------------------------------------------------------------------
// Results layout — rendered when a valid Case is available
// ---------------------------------------------------------------------------

interface ResultsLayoutProps {
  caseResult: CaseResult;
}

function ResultsLayout({ caseResult }: ResultsLayoutProps) {
  const { id, prediction, recommendation } = caseResult;

  // CI width above 0.35 means the model could not pin a probability — surface
  // a caution so the operator knows the point estimate is brittle.
  const ciWidth = prediction.ciUpper - prediction.ciLower;
  const wideCi = ciWidth > 0.35;

  return (
    <main className="mx-auto max-w-3xl p-6 sm:p-8 space-y-6">
      {/* Breadcrumb */}
      <nav aria-label="Breadcrumb" className="text-sm text-muted-foreground">
        <Link href="/cases" className="hover:text-foreground">Cases</Link>
        <span aria-hidden="true"> / </span>
        <span aria-current="page">Case Analysis</span>
      </nav>

      <h1 className="text-3xl font-bold tracking-tight">Case Analysis</h1>

      {/* Card #1 — P(win) header strip */}
      <Card>
        <CardHeader>
          <CardTitle>Outcome Probability</CardTitle>
          <CardDescription>
            Model version <span className="font-mono text-xs">{shortModelVersion(prediction.modelVersion)}</span>
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-2">
          <p
            className="text-5xl font-extrabold tabular-nums"
            aria-label={`P win ${fmtPercent(prediction.pWin)}`}
          >
            {fmtPercent(prediction.pWin)}
          </p>
          <p className="text-sm text-muted-foreground">
            90% confidence interval{" "}
            <span className="font-mono">
              [{prediction.ciLower.toFixed(2)}, {prediction.ciUpper.toFixed(2)}]
            </span>
          </p>
          {wideCi && (
            <p
              role="status"
              className="mt-2 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-900"
            >
              <strong>Low confidence:</strong> the {Math.round(ciWidth * 100)}-point
              CI width means the point estimate above is brittle. Treat the
              recommendation as directional, not definitive.
            </p>
          )}
        </CardContent>
      </Card>

      {/* Card #2 — Recommendation with server-computed reasoning bullets */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-3">
            Recommendation
            <Badge variant={badgeVariantForKind(recommendation.kind)}>
              {recommendation.kind}
            </Badge>
            {/* S6.4 — Confidence band sits next to the kind badge so the
                UI conveys "Settle (high conf)" / "Settle (borderline)" at
                a glance. */}
            <Badge
              variant="outline"
              className="text-xs font-normal"
              aria-label={`Confidence band: ${recommendation.confidence}`}
            >
              {recommendation.confidence.toLowerCase()} confidence
            </Badge>
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <ul
            className="space-y-2 list-disc list-inside text-sm"
            aria-label="Reasoning"
          >
            {recommendation.rationaleBullets.map((bullet, i) => (
              <li key={i}>{formatBulletAmounts(bullet)}</li>
            ))}
          </ul>
          {/* S6.4 — Counter-recommendation panel.  Only rendered when the
              backend marked confidence as "Low" (CI width ≥ 0.20); see
              decision-arith::recommend.  Operators see what the call would
              be at each end of the 90% CI so they can judge sensitivity. */}
          {recommendation.counterRecommendation && (
            <aside
              className="rounded-md border border-amber-200 bg-amber-50 p-3 text-xs"
              aria-label="Counter-recommendation"
            >
              <p className="font-medium text-amber-900">
                Sensitivity: this recommendation may shift inside the
                confidence interval
              </p>
              <dl className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 text-amber-900">
                <dt>At lower CI bound</dt>
                <dd className="font-semibold">
                  {recommendation.counterRecommendation.kindAtCiLower}
                </dd>
                <dt>At upper CI bound</dt>
                <dd className="font-semibold">
                  {recommendation.counterRecommendation.kindAtCiUpper}
                </dd>
              </dl>
              <p className="mt-2 text-amber-800">
                {recommendation.counterRecommendation.note}
              </p>
            </aside>
          )}
        </CardContent>
      </Card>

      {/* Card #3 — Expected value comparison */}
      <Card>
        <CardHeader>
          <CardTitle>Expected Value Comparison</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 gap-6 text-center">
            <div>
              <p className="text-xs text-muted-foreground uppercase tracking-wide">
                Expected value at trial
              </p>
              <p className="text-2xl font-bold tabular-nums">
                {fmtDollar(recommendation.expectedValueTry)}
              </p>
            </div>
            <div>
              <p className="text-xs text-muted-foreground uppercase tracking-wide">
                Expected value at settlement
              </p>
              <p className="text-2xl font-bold tabular-nums">
                {fmtDollar(recommendation.expectedValueSettle)}
              </p>
            </div>
          </div>
          <p className="mt-4 text-xs text-muted-foreground">
            EV(trial) = P(win) × expected damages − litigation cost.
            EV(settlement) = expected damages × 40%.
          </p>
        </CardContent>
      </Card>

      {/* Action bar: PDF download + re-run prediction */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start">
        {/* PDF memo download (S4.6) */}
        <div className="flex flex-col gap-1">
          <a
            href={`/api/case/${id}/memo.pdf`}
            download
            className="inline-flex h-11 items-center justify-center rounded-md border border-input bg-background px-4 text-sm font-medium shadow-sm hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
          >
            Download memo (PDF)
          </a>
          <p className="text-xs text-muted-foreground">
            Includes the prediction, recommendation, and audit trail.
          </p>
        </div>

        {/* Re-run prediction (S4.7) */}
        <div className="flex flex-col gap-1">
          <RepredictButton caseId={id} />
          <p className="text-xs text-muted-foreground">
            Refreshes this case against the latest champion model.
          </p>
        </div>
      </div>

      {/* Prediction history disclosure (S4.7) */}
      <PredictionHistoryDisclosure caseId={id} />

      {/* S10.5 — Tier-A sources used disclosure.
          When `ideologyProvenance` is non-null (cases predicted after
          Sprint 10 landed), the ideology section names the exact source +
          release + term that fired at prediction time, frozen for this
          case. When null (legacy cases or operator-typed-only flows), it
          falls back to the S7.6 "available sources" copy. Tier-C is
          explicitly absent — the system never accepts those features. */}
      <section
        aria-labelledby="sources-heading"
        className="mt-8 rounded-md border bg-muted/20 p-4 text-xs leading-relaxed text-muted-foreground"
      >
        <h2 id="sources-heading" className="text-sm font-semibold text-foreground">
          Tier-A sources used
        </h2>
        <ul className="mt-2 list-disc space-y-1 pl-5">
          <li>
            <strong>Judge severity:</strong> per-court win rate for the
            responding party, computed over our CourtListener opinion
            corpus (`judges.bio.severity_proxy`). Operator-typed when no
            opinion text was supplied at intake.
          </li>
          {caseResult.ideologyProvenance ? (
            <li>
              <strong>Ideology distance:</strong>{" "}
              {(() => {
                const p = caseResult.ideologyProvenance!;
                const label =
                  p.source === "martin_quinn"
                    ? "Martin-Quinn dynamic ideal-point"
                    : p.source === "judicial_common_space"
                    ? "Judicial Common Space (JCS)"
                    : p.source === "bonica_dime"
                    ? "Bonica DIME cfscore"
                    : p.source;
                return (
                  <>
                    <strong>{label}</strong>
                    {p.raw_score != null && (
                      <>
                        {" "}— raw score{" "}
                        <span className="font-mono">{p.raw_score.toFixed(3)}</span>
                      </>
                    )}
                    {p.term != null && (
                      <>
                        {" "}— term <span className="font-mono">{p.term}</span>
                      </>
                    )}
                    {p.release && (
                      <>
                        {" "}— release{" "}
                        <span className="font-mono">{p.release}</span>
                      </>
                    )}
                    . Snapshot taken{" "}
                    <span className="font-mono">{p.as_of_date}</span>; this
                    case&apos;s recommendation will continue to cite this
                    vintage even after the source updates.
                  </>
                );
              })()}
            </li>
          ) : (
          <li>
            <strong>Ideology distance:</strong> three Tier-A sources, in
            precedence order:
            <ul className="mt-1 list-[circle] space-y-1 pl-5">
              <li>
                <strong>Martin-Quinn</strong> dynamic ideal-points
                (`judges.bio.mqs.latest_score`), scaled from the
                roughly [-6, 6] voting-record space to [0, 1] around a
                neutral anchor. <em>Preferred when available</em> —
                voting-record-based, closer to &ldquo;how the judge
                rules&rdquo;. SCOTUS only.
                Methodology:{" "}
                <a className="underline" href="https://mqscores.lsa.umich.edu/" rel="noreferrer">
                  Martin &amp; Quinn (2002), updated annually
                </a>
                .
              </li>
              <li>
                <strong>Judicial Common Space (JCS)</strong>{" "}
                (`judges.bio.jcs.score`). Extends Martin-Quinn beyond
                SCOTUS to federal Circuit and District judges via
                Epstein/Martin/Quinn/Segal joint-scaling. Used when MQ
                isn&apos;t available.
                Methodology:{" "}
                <a className="underline" href="https://epstein.wustl.edu/judicial-common-space" rel="noreferrer">
                  Epstein, Martin, Quinn &amp; Segal (2007)
                </a>
                .
              </li>
              <li>
                <strong>Bonica DIME</strong>{" "}
                (`judges.bio.dime.cfscore`), scaled from the
                campaign-finance [-2, 2] space to [0, 1]. Used when
                neither MQ nor JCS has the judge. Broadest coverage,
                including state high courts.
                Methodology:{" "}
                <a className="underline" href="https://data.stanford.edu/dime" rel="noreferrer">
                  Bonica, Adam. DIME, Stanford
                </a>
                .
              </li>
            </ul>
            Falls back to operator-typed when no source has the judge.
          </li>
          )}
          <li>
            <strong>Attorney win rate, materiality score, procedural
            motions:</strong> operator-typed.
          </li>
          <li>
            <strong>Tier-C (party-identifying) features:</strong> never used.
            Enforced in the type system, the GraphQL schema, and the ML
            service&apos;s allowlist.
          </li>
        </ul>
      </section>
    </main>
  );
}

// ---------------------------------------------------------------------------
// Exported component — page.tsx passes the server-fetched Case (or null)
// ---------------------------------------------------------------------------

interface ResultsViewProps {
  caseResult: CaseResult | null;
}

export function ResultsView({ caseResult }: ResultsViewProps) {
  if (!caseResult) return <EmptyState />;
  return <ResultsLayout caseResult={caseResult} />;
}
