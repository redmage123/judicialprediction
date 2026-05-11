/**
 * CasesTable — S4.5 (JP-59)
 *
 * Purely presentational: renders the paginated cases list, recommendation
 * badges, and Previous/Next pagination controls.
 *
 * Layout:
 *  - sm+      : table with date / type / jurisdiction / P(win) / recommendation / View
 *  - below sm : card list with the same fields stacked vertically
 *
 * Pagination uses <Link> for the enabled state and a disabled <button> for the
 * disabled state (semantically correct: links navigate, disabled buttons
 * communicate "not available without JS involvement").
 *
 * Sprint-5 follow-ups:
 *  - Client-side sort by P(win) and recommendation kind
 *  - Filter by date range
 *  - CSV export
 */

import Link from "next/link";
import { Card, CardContent } from "@/components/ui/card";
import type { CaseConnection } from "@/lib/queries/predict";

// ---------------------------------------------------------------------------
// Badge styles keyed by recommendation kind.
// Try    = decisive action, go to court  → blue
// Settle = avoid risk, accept settlement → green
// Borderline = unclear, needs partner input → amber
// ---------------------------------------------------------------------------

const BADGE_CLASS: Record<string, string> = {
  Try: "bg-blue-100 text-blue-800 border border-blue-200",
  Settle: "bg-emerald-100 text-emerald-800 border border-emerald-200",
  Borderline: "bg-amber-100 text-amber-800 border border-amber-200",
};

const DATE_FMT = new Intl.DateTimeFormat("en-US", {
  year: "numeric",
  month: "short",
  day: "numeric",
});

function formatDate(iso: string): string {
  try {
    return DATE_FMT.format(new Date(iso));
  } catch {
    return iso;
  }
}

// inputFeatures is a JSON scalar from the gateway, so keys are snake_case.
function getCaseType(features: Record<string, unknown>): string {
  return String(features.case_type ?? features.caseType ?? "");
}

function getJurisdiction(features: Record<string, unknown>): string {
  return String(features.jurisdiction ?? "");
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface CasesTableProps {
  connection: CaseConnection;
  /** Current page offset (number of rows already shown before this page). */
  offset: number;
  pageSize: number;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function CasesTable({ connection, offset, pageSize }: CasesTableProps) {
  const { nodes, totalCount, nextOffset } = connection;

  // ---- empty state --------------------------------------------------------
  if (totalCount === 0) {
    return (
      <main className="container mx-auto py-12 px-4">
        <h1 className="text-2xl font-semibold tracking-tight mb-8">Cases</h1>
        <div className="text-center py-20 border border-dashed border-slate-200 rounded-lg">
          <p className="text-slate-500 text-base mb-4">No cases yet.</p>
          <Link
            href="/case/new"
            className="inline-flex h-11 items-center px-5 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90"
          >
            Submit your first case
          </Link>
        </div>
      </main>
    );
  }

  const fromRow = offset + 1;
  const toRow = offset + nodes.length;
  const prevOffset = Math.max(0, offset - pageSize);

  return (
    <main className="container mx-auto py-8 px-4 max-w-5xl">
      {/* Page header */}
      <div className="flex flex-wrap items-center justify-between gap-3 mb-6">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Cases</h1>
          <p className="text-sm text-muted-foreground">{`Showing ${fromRow}–${toRow} of ${totalCount}`}</p>
        </div>
        <Link
          href="/case/new"
          className="inline-flex h-11 items-center px-5 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90"
        >
          New case
        </Link>
      </div>

      {/* Desktop / tablet table */}
      <Card className="hidden sm:block">
        <CardContent className="p-0">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b bg-slate-50 text-slate-600 text-xs uppercase tracking-wider">
                <th scope="col" className="px-4 py-3 text-left font-medium">Date filed</th>
                <th scope="col" className="px-4 py-3 text-left font-medium">Case type</th>
                <th scope="col" className="px-4 py-3 text-left font-medium">Jurisdiction</th>
                <th scope="col" className="px-4 py-3 text-right font-medium">P(win) %</th>
                <th scope="col" className="px-4 py-3 text-left font-medium">Recommendation</th>
                <th scope="col" className="px-4 py-3 text-right font-medium"><span className="sr-only">Open case</span></th>
              </tr>
            </thead>
            <tbody>
              {nodes.map((c) => {
                const features = (c.inputFeatures ?? {}) as Record<string, unknown>;
                const badgeClass = BADGE_CLASS[c.recommendation.kind] ?? BADGE_CLASS.Borderline;
                return (
                  <tr key={c.id} className="border-b last:border-0 hover:bg-slate-50 transition-colors">
                    <td className="px-4 py-3 text-slate-700 whitespace-nowrap">{formatDate(c.createdAt)}</td>
                    <td className="px-4 py-3 capitalize">{getCaseType(features) || "—"}</td>
                    <td className="px-4 py-3">{getJurisdiction(features)}</td>
                    <td className="px-4 py-3 text-right font-mono tabular-nums">{Math.round(c.prediction.pWin * 100)}%</td>
                    <td className="px-4 py-3">
                      <span className={`inline-block px-2.5 py-1 rounded text-xs font-semibold ${badgeClass}`}>
                        {c.recommendation.kind}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-right">
                      <Link
                        href={`/case/${c.id}`}
                        className="inline-flex h-9 items-center justify-center rounded-md border border-input bg-background px-4 text-sm font-medium hover:bg-accent"
                      >
                        Open
                      </Link>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </CardContent>
      </Card>

      {/* Mobile card list (fixes the table overflow heuristic finding) */}
      <ul className="sm:hidden space-y-3" aria-label="Cases">
        {nodes.map((c) => {
          const features = (c.inputFeatures ?? {}) as Record<string, unknown>;
          const badgeClass = BADGE_CLASS[c.recommendation.kind] ?? BADGE_CLASS.Borderline;
          return (
            <li key={c.id}>
              <Link
                href={`/case/${c.id}`}
                className="block rounded-md border border-slate-200 bg-card p-4 hover:bg-slate-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="text-xs text-slate-500">{formatDate(c.createdAt)}</p>
                    <p className="mt-1 text-sm font-medium capitalize">
                      {getCaseType(features) || "Case"} · {getJurisdiction(features)}
                    </p>
                  </div>
                  <span className={`shrink-0 inline-block px-2.5 py-1 rounded text-xs font-semibold ${badgeClass}`}>
                    {c.recommendation.kind}
                  </span>
                </div>
                <div className="mt-3 flex items-baseline gap-2">
                  <span className="text-2xl font-bold tabular-nums">{Math.round(c.prediction.pWin * 100)}%</span>
                  <span className="text-xs text-slate-500 uppercase tracking-wide">P(win)</span>
                </div>
              </Link>
            </li>
          );
        })}
      </ul>

      {/* Pagination footer */}
      <div className="flex items-center justify-between mt-4 text-sm text-slate-600">
        <span>{`Showing ${fromRow}–${toRow} of ${totalCount}`}</span>
        <nav aria-label="Pagination" className="flex gap-2">
          {offset > 0 ? (
            <Link
              href={`/cases?offset=${prevOffset}`}
              className="inline-flex h-9 items-center px-3 rounded border border-slate-300 hover:bg-slate-50"
              aria-label="Previous page"
            >
              Previous
            </Link>
          ) : (
            <button
              disabled
              aria-label="Previous page"
              className="inline-flex h-9 items-center px-3 rounded border border-slate-200 text-slate-400 cursor-not-allowed"
            >
              Previous
            </button>
          )}

          {nextOffset !== null ? (
            <Link
              href={`/cases?offset=${nextOffset}`}
              className="inline-flex h-9 items-center px-3 rounded border border-slate-300 hover:bg-slate-50"
              aria-label="Next page"
            >
              Next
            </Link>
          ) : (
            <button
              disabled
              aria-label="Next page"
              className="inline-flex h-9 items-center px-3 rounded border border-slate-200 text-slate-400 cursor-not-allowed"
            >
              Next
            </button>
          )}
        </nav>
      </div>
    </main>
  );
}
