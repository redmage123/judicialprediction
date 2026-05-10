// S4.4 (JP-58): converted from a stateful client island (sessionStorage reader)
// to a pure presentational component.  Data is fetched server-side in page.tsx
// and passed here as a prop.  No sessionStorage, no useEffect, no useQuery.
//
// lib/recommend.ts is no longer called from this component — the server-computed
// recommendation from createCase / case(id) is used directly.
// See lib/recommend.ts deprecation notice.

import Link from "next/link";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import type { CaseResult } from "@/lib/queries/predict";

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
  const { prediction, recommendation } = caseResult;

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
