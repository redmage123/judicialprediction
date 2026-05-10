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
import { useMutation, useQuery } from "@apollo/client";
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

function badgeVariantForKind(
  kind: string
): "default" | "secondary" | "warning" {
  if (kind === "Try") return "default";
  if (kind === "Settle") return "secondary";
  return "warning";
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
      onClick={() => repredictCase()}
      disabled={loading}
      aria-label="Re-run with latest model"
      className="inline-flex items-center justify-center rounded-md border border-input bg-background px-4 py-2 text-sm font-medium shadow-sm hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50"
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

  return (
    <main className="mx-auto max-w-3xl p-8 space-y-6">
      <h1 className="text-3xl font-bold tracking-tight">Case Analysis</h1>

      {/* Card #1 — P(win) header strip */}
      <Card>
        <CardHeader>
          <CardTitle>Outcome Probability</CardTitle>
          <CardDescription>
            Model:{" "}
            <span className="font-mono text-xs">{prediction.modelVersion}</span>
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-1">
          <p
            className="text-5xl font-extrabold tabular-nums"
            aria-label={`P win ${fmtPercent(prediction.pWin)}`}
          >
            {fmtPercent(prediction.pWin)}
          </p>
          <p className="text-sm text-muted-foreground">
            90% CI{" "}
            <span className="font-mono">
              [{prediction.ciLower.toFixed(2)}, {prediction.ciUpper.toFixed(2)}]
            </span>
          </p>
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
          </CardTitle>
        </CardHeader>
        <CardContent>
          <ul
            className="space-y-2 list-disc list-inside text-sm"
            aria-label="Reasoning"
          >
            {recommendation.rationaleBullets.map((bullet, i) => (
              <li key={i}>{bullet}</li>
            ))}
          </ul>
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
            className="inline-flex items-center justify-center rounded-md border border-input bg-background px-4 py-2 text-sm font-medium shadow-sm hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
          >
            Download memo (PDF)
          </a>
          <p className="text-xs text-muted-foreground">
            PDF includes the audit trail and is signed by the gateway
            model_version. Sprint-5 adds full statutory citations.
          </p>
        </div>

        {/* Re-run prediction (S4.7) */}
        <div className="flex flex-col gap-1">
          <RepredictButton caseId={id} />
          <p className="text-xs text-muted-foreground">
            Fetches the latest champion model and updates this case.
            Recommendation is preserved; Sprint-5 will re-run decision-arith
            when you supply updated damages.
          </p>
        </div>
      </div>

      {/* Prediction history disclosure (S4.7) */}
      <PredictionHistoryDisclosure caseId={id} />

      {/* Demo limitations disclosure */}
      <p className="text-xs text-muted-foreground border-t pt-4">
        <strong>Demo limitations:</strong> Settlement value uses a 40% damages
        anchor; cost-engine integration and real BATNA modelling come in
        Sprint 5.
      </p>
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
