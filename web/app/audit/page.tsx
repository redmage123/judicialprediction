// S6.12 — operator-facing `/audit` page.
//
// RSC that fetches the auditEvents GraphQL query from api-gateway server-side
// using the same pattern as /cases (reads jp_session cookie, attaches the
// Authorization header, no client-side Apollo).  Behind the `admin` role gate
// enforced by middleware.ts.
//
// Extends S4.9 (Django admin audit-log viewer) into the operator UI.  Tenant
// isolation is enforced server-side via Postgres RLS — this page does not
// re-filter rows.
//
// Pagination: ?offset searchParam (default 0), page size = 25.

import type { Metadata } from "next";
import { cookies } from "next/headers";
import { AuditTable } from "./audit-table";
import type { AuditConnection } from "@/lib/queries/audit";

export const metadata: Metadata = {
  title: "Audit log — JudicialPredict",
};

export const dynamic = "force-dynamic";

const GATEWAY_URL =
  process.env.GATEWAY_INTERNAL_URL ?? "http://localhost:4000";
const PAGE_SIZE = 25;

const EMPTY_CONNECTION: AuditConnection = {
  nodes: [],
  totalCount: 0,
  nextOffset: null,
};

// ---------------------------------------------------------------------------
// Data fetcher
// ---------------------------------------------------------------------------

async function fetchPage(offset: number): Promise<AuditConnection> {
  const cookieStore = await cookies();
  const token = cookieStore.get("jp_session")?.value;

  // Query is duplicated as a plain string here (vs. importing AUDIT_EVENTS)
  // because the RSC fetch uses raw GraphQL-over-HTTP without Apollo — same
  // pattern /cases uses.
  const query = `
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

  try {
    const resp = await fetch(`${GATEWAY_URL}/graphql`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify({
        query,
        variables: { limit: PAGE_SIZE, offset },
      }),
      cache: "no-store",
    });

    if (!resp.ok) {
      console.error("[audit-page] gateway returned", resp.status);
      return EMPTY_CONNECTION;
    }

    const json = (await resp.json()) as {
      data?: { auditEvents?: AuditConnection };
      errors?: unknown[];
    };

    if (json.errors?.length) {
      console.error("[audit-page] GraphQL errors:", json.errors);
      return EMPTY_CONNECTION;
    }

    return json.data?.auditEvents ?? EMPTY_CONNECTION;
  } catch (err) {
    console.error("[audit-page] gateway fetch failed:", err);
    return EMPTY_CONNECTION;
  }
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

type Props = {
  searchParams: Promise<{ offset?: string }>;
};

export default async function AuditPage({ searchParams }: Props) {
  const { offset: offsetParam } = await searchParams;
  const offset = Math.max(0, parseInt(offsetParam ?? "0", 10) || 0);
  const connection = await fetchPage(offset);

  return (
    <main className="container mx-auto pb-8 px-4 max-w-6xl">
      <div className="flex flex-wrap items-center justify-between gap-3 my-6">
        <h1 className="text-xl font-semibold tracking-tight">Audit log</h1>
        <p className="text-sm text-muted-foreground">
          Read-only · most-recent-first · tenant-scoped via RLS
        </p>
      </div>
      <AuditTable
        connection={connection}
        offset={offset}
        pageSize={PAGE_SIZE}
      />
    </main>
  );
}
