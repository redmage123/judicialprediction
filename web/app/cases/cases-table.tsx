/**
 * CasesTable — S4.5 (JP-59)
 *
 * Purely presentational: renders the paginated cases list, recommendation
 * badges, and Previous/Next pagination controls. Accepts pre-fetched data as
 * props so it can be tested without mocking fetch or Next.js internals.
 *
 * Pagination uses <Link> for the enabled state and a disabled <button> for the
 * disabled state (semantically correct: links navigate, disabled buttons
 * communicate "not available without JS involvement").
 *
 * Sprint-5 follow-ups:
 * - Client-side sort by P(win) and recommendation kind
 * - Filter by date range
 * - CSV export
 */

import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { CaseConnection } from "@/lib/queries/predict";

// ---------------------------------------------------------------------------
// Badge styles keyed by recommendation kind
// ---------------------------------------------------------------------------

const BADGE_CLASS: Record<string, string> = {
  Try: "bg-blue-100 text-blue-800 border border-blue-200",
  Settle: "bg-slate-100 text-slate-700 border border-slate-200",
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
            className="inline-flex items-center px-4 py-2 rounded-md bg-slate-900 text-white text-sm font-medium hover:bg-slate-700"
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
    <main className="container mx-auto py-8 px-4">
      {/* Page header */}
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-semibold tracking-tight">Cases</h1>
        <Link
          href="/case/new"
          className="inline-flex items-center px-4 py-2 rounded-md bg-slate-900 text-white text-sm font-medium hover:bg-slate-700"
        >
          New case
        </Link>
      </div>

      {/* Table card */}
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium text-slate-500">
            {`Showing ${fromRow}–${toRow} of ${totalCount}`}
          </CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b bg-slate-50 text-slate-600 text-xs uppercase tracking-wider">
                  <th scope="col" className="px-4 py-3 text-left font-medium">
                    Date filed
                  </th>
                  <th scope="col" className="px-4 py-3 text-left font-medium">
                    Case type
                  </th>
                  <th scope="col" className="px-4 py-3 text-left font-medium">
                    Jurisdiction
                  </th>
                  <th scope="col" className="px-4 py-3 text-right font-medium">
                    P(win) %
                  </th>
                  <th scope="col" className="px-4 py-3 text-left font-medium">
                    Recommendation
                  </th>
                  <th scope="col" className="px-4 py-3 text-left font-medium sr-only">
                    Action
                  </th>
                </tr>
              </thead>
              <tbody>
                {nodes.map((c) => {
                  const badgeClass =
                    BADGE_CLASS[c.recommendation.kind] ?? BADGE_CLASS.Borderline;
                  return (
                    <tr
                      key={c.id}
                      className="border-b last:border-0 hover:bg-slate-50 transition-colors"
                    >
                      <td className="px-4 py-3 text-slate-700 whitespace-nowrap">
                        {formatDate(c.createdAt)}
                      </td>
                      <td className="px-4 py-3 capitalize">
                        {c.inputFeatures.caseType}
                      </td>
                      <td className="px-4 py-3">{c.inputFeatures.jurisdiction}</td>
                      <td className="px-4 py-3 text-right font-mono tabular-nums">
                        {Math.round(c.prediction.pWin * 100)}%
                      </td>
                      <td className="px-4 py-3">
                        <span
                          className={`inline-block px-2 py-0.5 rounded text-xs font-medium ${badgeClass}`}
                        >
                          {c.recommendation.kind}
                        </span>
                      </td>
                      <td className="px-4 py-3">
                        <Link
                          href={`/case/${c.id}`}
                          className="text-slate-700 underline underline-offset-2 hover:text-slate-900 text-xs"
                        >
                          View
                        </Link>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>

      {/* Pagination footer */}
      <div className="flex items-center justify-between mt-4 text-sm text-slate-600">
        <span>{`Showing ${fromRow}–${toRow} of ${totalCount}`}</span>
        <nav aria-label="Pagination" className="flex gap-2">
          {offset > 0 ? (
            <Link
              href={`/cases?offset=${prevOffset}`}
              className="px-3 py-1.5 rounded border border-slate-300 hover:bg-slate-50"
              aria-label="Previous page"
            >
              Previous
            </Link>
          ) : (
            <button
              disabled
              aria-label="Previous page"
              className="px-3 py-1.5 rounded border border-slate-200 text-slate-400 cursor-not-allowed"
            >
              Previous
            </button>
          )}

          {nextOffset !== null ? (
            <Link
              href={`/cases?offset=${nextOffset}`}
              className="px-3 py-1.5 rounded border border-slate-300 hover:bg-slate-50"
              aria-label="Next page"
            >
              Next
            </Link>
          ) : (
            <button
              disabled
              aria-label="Next page"
              className="px-3 py-1.5 rounded border border-slate-200 text-slate-400 cursor-not-allowed"
            >
              Next
            </button>
          )}
        </nav>
      </div>
    </main>
  );
}
