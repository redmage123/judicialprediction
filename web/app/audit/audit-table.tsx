/**
 * AuditTable - S6.12.
 *
 * Purely presentational: paginated, read-only table of audit events.  Mirrors
 * the styling and pagination semantics of CasesTable so the two operator
 * surfaces feel consistent.
 *
 * Columns (matches S4.9 Django admin viewer + a synthetic target column):
 *   - timestamp (UTC)
 *   - actor
 *   - action
 *   - target          (table_name + row_pk, composed on the gateway)
 *   - reason / status (audit_log.reason_code: ok/err/timeout/rate_limit)
 *   - latency (ms)
 *
 * IP and request-id were called out in the S6.12 ticket but are not stored
 * on audit_log today (only subject_id, table_name, row_pk, action,
 * reason_code, ts, latency_ms, cost_micros are persisted).  Adding those
 * columns is a Sprint-7 follow-up.
 */

import Link from "next/link";
import { Card, CardContent } from "@/components/ui/card";
import type { AuditConnection } from "@/lib/queries/audit";

// ---------------------------------------------------------------------------
// Reason-code to badge styling.  Matches the AuditStatus enum on the Rust side
// (audit_recorder::AuditStatus): ok / err / timeout / rate_limit.
// ---------------------------------------------------------------------------

const REASON_BADGE_CLASS: Record<string, string> = {
  ok: "bg-emerald-100 text-emerald-800 border border-emerald-200",
  err: "bg-rose-100 text-rose-800 border border-rose-200",
  timeout: "bg-amber-100 text-amber-800 border border-amber-200",
  rate_limit: "bg-amber-100 text-amber-800 border border-amber-200",
};

// ---------------------------------------------------------------------------
// Timestamp formatting: UTC so audit rows stay comparable across operator
// time zones.  Falls back to the raw ISO string if parsing fails.
// ---------------------------------------------------------------------------

const TS_FMT = new Intl.DateTimeFormat("en-US", {
  year: "numeric",
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
  timeZone: "UTC",
});

function formatTs(iso: string): string {
  try {
    return TS_FMT.format(new Date(iso)) + " UTC";
  } catch {
    return iso;
  }
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface AuditTableProps {
  connection: AuditConnection;
  /** Current page offset (number of rows already shown before this page). */
  offset: number;
  pageSize: number;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function AuditTable({ connection, offset, pageSize }: AuditTableProps) {
  const { nodes, totalCount, nextOffset } = connection;

  if (totalCount === 0) {
    return (
      <div className="text-center py-20 border border-dashed border-slate-200 rounded-lg">
        <p className="text-slate-500 text-base">No audit events yet.</p>
      </div>
    );
  }

  const fromRow = offset + 1;
  const toRow = offset + nodes.length;
  const prevOffset = Math.max(0, offset - pageSize);

  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-3 mb-4">
        <h2 className="text-lg font-semibold tracking-tight">Recent events</h2>
        <p className="text-sm text-muted-foreground">
          {"Showing " + fromRow + "-" + toRow + " of " + totalCount}
        </p>
      </div>

      <Card>
        <CardContent className="p-0 overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b bg-slate-50 text-slate-600 text-xs uppercase tracking-wider">
                <th scope="col" className="px-4 py-3 text-left font-medium whitespace-nowrap">
                  Timestamp (UTC)
                </th>
                <th scope="col" className="px-4 py-3 text-left font-medium">Actor</th>
                <th scope="col" className="px-4 py-3 text-left font-medium">Action</th>
                <th scope="col" className="px-4 py-3 text-left font-medium">Target</th>
                <th scope="col" className="px-4 py-3 text-left font-medium">Status</th>
                <th scope="col" className="px-4 py-3 text-right font-medium">Latency</th>
              </tr>
            </thead>
            <tbody>
              {nodes.map((row) => {
                const badge =
                  row.reasonCode != null
                    ? REASON_BADGE_CLASS[row.reasonCode] ??
                      "bg-slate-100 text-slate-800 border border-slate-200"
                    : null;
                return (
                  <tr
                    key={row.id}
                    className="border-b last:border-0 hover:bg-slate-50 transition-colors"
                  >
                    <td className="px-4 py-3 text-slate-700 whitespace-nowrap font-mono tabular-nums">
                      {formatTs(row.ts)}
                    </td>
                    <td className="px-4 py-3 break-all">{row.actor ?? "-"}</td>
                    <td className="px-4 py-3 font-mono text-xs">{row.action}</td>
                    <td className="px-4 py-3 font-mono text-xs break-all">{row.target}</td>
                    <td className="px-4 py-3">
                      {badge != null && row.reasonCode != null ? (
                        <span
                          className={"inline-block px-2 py-0.5 rounded text-xs font-semibold " + badge}
                        >
                          {row.reasonCode}
                        </span>
                      ) : (
                        <span className="text-slate-400">-</span>
                      )}
                    </td>
                    <td className="px-4 py-3 text-right font-mono tabular-nums whitespace-nowrap">
                      {row.latencyMs != null ? row.latencyMs + " ms" : "-"}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </CardContent>
      </Card>

      {/* Pagination footer */}
      <div className="flex items-center justify-between mt-4 text-sm text-slate-600">
        <span>{"Showing " + fromRow + "-" + toRow + " of " + totalCount}</span>
        <nav aria-label="Pagination" className="flex gap-2">
          {offset > 0 ? (
            <Link
              href={"/audit?offset=" + prevOffset}
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
              href={"/audit?offset=" + nextOffset}
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
    </>
  );
}
