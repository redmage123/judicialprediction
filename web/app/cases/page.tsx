// S4.5 (JP-59): /cases page — authenticated operator's case list.
//
// RSC that fetches `listCases` from api-gateway server-side (same pattern
// as /case/[id]/page.tsx: reads jp_session cookie, attaches Authorization
// header, calls gateway directly — no client-side Apollo needed).
//
// Pagination: reads ?offset from searchParams (default 0), page size = 20.
// Renders CasesTable (presentational component) with the fetched connection.

import type { Metadata } from "next";
import { cookies } from "next/headers";
import { CasesTable } from "./cases-table";
import { CaseStatsCards, type CaseStats } from "./case-stats-cards";
import type { CaseConnection } from "@/lib/queries/predict";

export const metadata: Metadata = {
  title: "Cases — JudicialPredict",
};

export const dynamic = "force-dynamic";

const GATEWAY_URL =
  process.env.GATEWAY_INTERNAL_URL ?? "http://localhost:4000";
const PAGE_SIZE = 20;

const EMPTY_CONNECTION: CaseConnection = {
  nodes: [],
  totalCount: 0,
  nextOffset: null,
};

const EMPTY_STATS: CaseStats = {
  totalCount: 0,
  settleCount: 0,
  tryCount: 0,
  borderlineCount: 0,
  avgPWin: null,
  lastSevenDaysCount: 0,
};

// ---------------------------------------------------------------------------
// Data fetcher
// ---------------------------------------------------------------------------

async function fetchPage(offset: number): Promise<{ connection: CaseConnection; stats: CaseStats }> {
  const cookieStore = await cookies();
  const token = cookieStore.get("jp_session")?.value;

  // Single round-trip: list + aggregate counters.  Both honor the same tenant
  // claim, so the dashboard numbers always match the visible rows.
  const query = `
    query CasesDashboard($limit: Int, $offset: Int) {
      listCases(limit: $limit, offset: $offset) {
        nodes {
          id
          inputFeatures
          prediction { pWin }
          recommendation { kind }
          createdAt
          dateFiled
          createdBy
        }
        totalCount
        nextOffset
      }
      caseStats {
        totalCount
        settleCount
        tryCount
        borderlineCount
        avgPWin
        lastSevenDaysCount
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
      console.error("[cases-page] gateway returned", resp.status);
      return { connection: EMPTY_CONNECTION, stats: EMPTY_STATS };
    }

    const json = (await resp.json()) as {
      data?: { listCases?: CaseConnection; caseStats?: CaseStats };
      errors?: unknown[];
    };

    if (json.errors?.length) {
      console.error("[cases-page] GraphQL errors:", json.errors);
      return { connection: EMPTY_CONNECTION, stats: EMPTY_STATS };
    }

    return {
      connection: json.data?.listCases ?? EMPTY_CONNECTION,
      stats: json.data?.caseStats ?? EMPTY_STATS,
    };
  } catch (err) {
    console.error("[cases-page] gateway fetch failed:", err);
    return { connection: EMPTY_CONNECTION, stats: EMPTY_STATS };
  }
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

type Props = {
  searchParams: Promise<{ offset?: string }>;
};

export default async function CasesPage({ searchParams }: Props) {
  const { offset: offsetParam } = await searchParams;
  const offset = Math.max(0, parseInt(offsetParam ?? "0", 10) || 0);
  const { connection, stats } = await fetchPage(offset);
  return (
    <>
      <CaseStatsCards stats={stats} />
      <CasesTable connection={connection} offset={offset} pageSize={PAGE_SIZE} />
    </>
  );
}
