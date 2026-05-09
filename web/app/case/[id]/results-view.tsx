"use client";

// JP-44 (/case/[id] results view) — S3.3.
//
// Reads the prediction stashed in sessionStorage under "case:<uuid>" by the S3.2 intake form
// (app/case/new/intake-form.tsx), computes a settle/try/borderline recommendation via
// lib/recommend.ts (TypeScript mirror of rust/decision-arith/src/recommend.rs), and
// renders the result using shadcn/ui Card + Badge components.
//
// Sprint-3 wave-3 follow-up: replace lib/recommend.ts with a `recommend` GraphQL query
// that calls Rust decision-arith over the wire, so the math lives in one place.
// The Rust file (rust/decision-arith/src/recommend.rs) is the source of truth for thresholds.

import { useEffect, useState } from "react";
import Link from "next/link";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { recommend, type Recommendation } from "@/lib/recommend";

// Sprint-3 placeholder litigation cost — Sprint-4 will wire the real cost-engine.
const DEMO_LITIGATION_COST = 50_000;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/**
 * Shape of the JSON stashed by the S3.2 intake form under `case:<uuid>` in sessionStorage.
 *
 * `expectedDamages` is set by the intake form from the user's damages input. If absent
 * (e.g. form was submitted before S3.2 landed), a conservative demo default is used.
 */
interface StoredResult {
  pWin: number;
  ciLower: number;
  ciUpper: number;
  coverage: number;
  modelVersion: string;
  predictedAtUnix: number;
  /** Optional: only present if the intake form stashed it (S3.2+). */
  expectedDamages?: number;
}

function isStoredResult(v: unknown): v is StoredResult {
  if (!v || typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  return (
    typeof o.pWin === "number" &&
    typeof o.ciLower === "number" &&
    typeof o.ciUpper === "number" &&
    typeof o.modelVersion === "string"
  );
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

function fmtPercent(p: number): string {
  return `${Math.round(p * 100)}%`;
}

function fmtDollar(n: number): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 0,
  }).format(n);
}

function badgeVariantForKind(
  kind: Recommendation["kind"]
): "default" | "secondary" | "warning" {
  if (kind === "Try") return "default";
  if (kind === "Settle") return "secondary";
  return "warning";
}

// ---------------------------------------------------------------------------
// Empty state — missing or expired sessionStorage entry
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
// Results layout — shown when sessionStorage contains valid prediction data
// ---------------------------------------------------------------------------

interface ResultsLayoutProps {
  stored: StoredResult;
}

function ResultsLayout({ stored }: ResultsLayoutProps) {
  // expectedDamages: from intake form if present; $250,000 demo default otherwise.
  const expectedDamages = stored.expectedDamages ?? 250_000;

  const rec = recommend(
    {
      pWin: stored.pWin,
      ciLower: stored.ciLower,
      ciUpper: stored.ciUpper,
      expectedDamages,
    },
    DEMO_LITIGATION_COST,
  );

  return (
    <main className="mx-auto max-w-3xl p-8 space-y-6">
      <h1 className="text-3xl font-bold tracking-tight">Case Analysis</h1>

      {/* Card #1 — P(win) header strip */}
      <Card>
        <CardHeader>
          <CardTitle>Outcome Probability</CardTitle>
          <CardDescription>
            Model:{" "}
            <span className="font-mono text-xs">{stored.modelVersion}</span>
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-1">
          <p
            className="text-5xl font-extrabold tabular-nums"
            aria-label={`P win ${fmtPercent(stored.pWin)}`}
          >
            {fmtPercent(stored.pWin)}
          </p>
          <p className="text-sm text-muted-foreground">
            90% CI{" "}
            <span className="font-mono">
              [{stored.ciLower.toFixed(2)}, {stored.ciUpper.toFixed(2)}]
            </span>
          </p>
        </CardContent>
      </Card>

      {/* Card #2 — Recommendation with 3 reasoning bullets */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-3">
            Recommendation
            <Badge variant={badgeVariantForKind(rec.kind)}>{rec.kind}</Badge>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <ul
            className="space-y-2 list-disc list-inside text-sm"
            aria-label="Reasoning"
          >
            {rec.bullets.map((bullet, i) => (
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
                {fmtDollar(rec.expectedValueTry)}
              </p>
            </div>
            <div>
              <p className="text-xs text-muted-foreground uppercase tracking-wide">
                Expected value at settlement
              </p>
              <p className="text-2xl font-bold tabular-nums">
                {fmtDollar(rec.expectedValueSettle)}
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
        anchor for Sprint 3; cost-engine integration and real BATNA modelling
        come in Sprint 4.
      </p>
    </main>
  );
}

// ---------------------------------------------------------------------------
// Exported client island — server component page.tsx renders this directly
// ---------------------------------------------------------------------------

interface ResultsViewProps {
  caseId: string;
}

type ViewState = "loading" | "empty" | StoredResult;

export function ResultsView({ caseId }: ResultsViewProps) {
  const [state, setState] = useState<ViewState>("loading");

  useEffect(() => {
    const key = `case:${caseId}`;
    try {
      const raw = sessionStorage.getItem(key);
      if (!raw) {
        setState("empty");
        return;
      }
      const parsed: unknown = JSON.parse(raw);
      if (isStoredResult(parsed)) {
        setState(parsed);
      } else {
        setState("empty");
      }
    } catch {
      // Malformed JSON or missing sessionStorage access — show empty state.
      setState("empty");
    }
  }, [caseId]);

  if (state === "loading") return null;
  if (state === "empty") return <EmptyState />;
  return <ResultsLayout stored={state} />;
}
