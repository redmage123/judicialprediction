/**
 * Audit log GraphQL contract — S6.12.
 *
 * Mirrors the auditEvents resolver added to api-gateway in this sprint and
 * gives the /audit RSC page a typed AuditConnection to render.  The column
 * surface matches S4.9's Django admin viewer 1:1 plus the synthetic
 * `target` field (table_name + row_pk composed on the gateway).
 */

import { gql } from "@apollo/client";

// ---------------------------------------------------------------------------
// GraphQL query
// ---------------------------------------------------------------------------

/** Paginated audit log read for the current tenant, most-recent-first. */
export const AUDIT_EVENTS = gql`
  query AuditEvents($limit: Int, $offset: Int) {
    auditEvents(limit: $limit, offset: $offset) {
      nodes {
        id
        tenantId
        actor
        action
        target
        reasonCode
        ts
        latencyMs
      }
      totalCount
      nextOffset
    }
  }
`;

// ---------------------------------------------------------------------------
// TypeScript types mirroring the GraphQL schema
// ---------------------------------------------------------------------------

/** One audit_log row, projected through the gateway. */
export interface AuditEvent {
  /** Stringified bigint primary key. */
  id: string;
  /** Tenant UUID the row was written under; null for tenant-agnostic events. */
  tenantId: string | null;
  /** Actor that triggered the event (operator UUID, email, or service principal). */
  actor: string | null;
  /** Fully-qualified action name (e.g. `case.create`, `predict_case_outcome`). */
  action: string;
  /** Composite `table_name:row_pk`, or just `table_name` when row-less. */
  target: string;
  /** Stable outcome code (`ok` / `err` / `timeout` / `rate_limit`). */
  reasonCode: string | null;
  /** ISO-8601 UTC timestamp (Postgres `ts` column, text-cast). */
  ts: string;
  /** Round-trip latency in milliseconds; null when not applicable. */
  latencyMs: number | null;
}

/** Paginated connection returned by auditEvents. */
export interface AuditConnection {
  nodes: AuditEvent[];
  totalCount: number;
  /** Present when a next page exists; null when this is the final page. */
  nextOffset: number | null;
}

export interface AuditEventsData {
  auditEvents: AuditConnection;
}

export interface AuditEventsVars {
  limit?: number;
  offset?: number;
}
