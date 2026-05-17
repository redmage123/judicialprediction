/**
 * CaseStatsCards — dashboard header shown above the case list.
 *
 * Renders one row of summary cards (total, recommendation breakdown, average
 * P(win), last seven days) plus a primary "New case" CTA.  Server-rendered
 * with data from `Query.caseStats` so the numbers always match the visible
 * rows.
 *
 * Empty state (no cases yet): a single full-width "Run your first analysis"
 * card with the CTA front and center.
 */

import Link from "next/link";
import { Card, CardContent } from "@/components/ui/card";

export interface CaseStats {
  totalCount: number;
  settleCount: number;
  tryCount: number;
  borderlineCount: number;
  avgPWin: number | null;
  lastSevenDaysCount: number;
}

function pct(num: number, total: number): string {
  if (total <= 0) return "0%";
  return `${Math.round((num / total) * 100)}%`;
}

function formatAvgPWin(v: number | null): string {
  if (v == null) return "—";
  return `${Math.round(v * 100)}%`;
}

export function CaseStatsCards({ stats }: { stats: CaseStats }) {
  // ---- empty state: nudge to first case -----------------------------------
  if (stats.totalCount === 0) {
    return (
      <section className="container mx-auto pt-8 px-4 max-w-5xl">
        <div className="flex flex-wrap items-end justify-between gap-3 mb-6">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight">Dashboard</h1>
            <p className="text-sm text-muted-foreground">
              Run your first case evaluation to populate this view.
            </p>
          </div>
          <Link
            href="/case/new"
            className="inline-flex h-11 items-center px-5 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90"
          >
            New case
          </Link>
        </div>
      </section>
    );
  }

  // ---- populated state: 4-card summary row --------------------------------
  return (
    <section className="container mx-auto pt-8 px-4 max-w-5xl">
      {/* Audit finding (2026-05-17): the champion model was trained on
          synthetic data, which is why early dashboards show every case
          collapsing to 50% / Settle.  Show this disclosure until S11
          retrains on a real corpus. */}
      <div
        role="status"
        className="mb-6 rounded-md border border-amber-300 bg-amber-50 px-4 py-3 text-xs text-amber-900"
      >
        <strong>Beta model:</strong> the current champion was trained on{" "}
        <span className="font-mono">synthetic_cases_v0.parquet</span>. Treat
        predictions as directional until the real-corpus retrain ships
        (tracked as Sprint&nbsp;11).
      </div>
      <div className="flex flex-wrap items-end justify-between gap-3 mb-6">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Dashboard</h1>
          <p className="text-sm text-muted-foreground">
            Cases analyzed for your firm, with recommendation breakdown and
            seven-day activity.
          </p>
        </div>
        <Link
          href="/case/new"
          className="inline-flex h-11 items-center px-5 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90"
        >
          New case
        </Link>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4 mb-6">
        {/* Total cases */}
        <Card>
          <CardContent className="p-4">
            <p className="text-xs uppercase tracking-wide text-muted-foreground">
              Total cases
            </p>
            <p className="mt-2 text-3xl font-bold tabular-nums">{stats.totalCount}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              All-time analyses for your firm
            </p>
          </CardContent>
        </Card>

        {/* Recommendation breakdown */}
        <Card>
          <CardContent className="p-4">
            <p className="text-xs uppercase tracking-wide text-muted-foreground">
              Recommendation mix
            </p>
            <ul className="mt-2 space-y-1 text-sm">
              <li className="flex items-baseline justify-between">
                <span className="inline-flex items-center gap-1.5">
                  <span aria-hidden className="h-2 w-2 rounded-full bg-emerald-500" />
                  Settle
                </span>
                <span className="tabular-nums font-medium">
                  {stats.settleCount}
                  <span className="ml-1 text-xs text-muted-foreground">
                    ({pct(stats.settleCount, stats.totalCount)})
                  </span>
                </span>
              </li>
              <li className="flex items-baseline justify-between">
                <span className="inline-flex items-center gap-1.5">
                  <span aria-hidden className="h-2 w-2 rounded-full bg-blue-500" />
                  Try
                </span>
                <span className="tabular-nums font-medium">
                  {stats.tryCount}
                  <span className="ml-1 text-xs text-muted-foreground">
                    ({pct(stats.tryCount, stats.totalCount)})
                  </span>
                </span>
              </li>
              <li className="flex items-baseline justify-between">
                <span className="inline-flex items-center gap-1.5">
                  <span aria-hidden className="h-2 w-2 rounded-full bg-amber-500" />
                  Borderline
                </span>
                <span className="tabular-nums font-medium">
                  {stats.borderlineCount}
                  <span className="ml-1 text-xs text-muted-foreground">
                    ({pct(stats.borderlineCount, stats.totalCount)})
                  </span>
                </span>
              </li>
            </ul>
          </CardContent>
        </Card>

        {/* Average P(win) */}
        <Card>
          <CardContent className="p-4">
            <p className="text-xs uppercase tracking-wide text-muted-foreground">
              Average P(win)
            </p>
            <p className="mt-2 text-3xl font-bold tabular-nums">
              {formatAvgPWin(stats.avgPWin)}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Mean predicted win probability
            </p>
          </CardContent>
        </Card>

        {/* Last 7 days */}
        <Card>
          <CardContent className="p-4">
            <p className="text-xs uppercase tracking-wide text-muted-foreground">
              Last 7 days
            </p>
            <p className="mt-2 text-3xl font-bold tabular-nums">{stats.lastSevenDaysCount}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {stats.lastSevenDaysCount === 1 ? "case analyzed" : "cases analyzed"}
            </p>
          </CardContent>
        </Card>
      </div>
    </section>
  );
}
