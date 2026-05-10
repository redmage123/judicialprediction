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

// ---------------------------------------------------------------------------
// Data fetcher
// ---------------------------------------------------------------------------

async function fetchCases(offset: number): Promise<CaseConnection> {
  const cookieStore = await cookies();
  const token = cookieStore.get("jp_session")?.value;

  const query = `
    query ListCases($limit: Int, $offset: Int) {
      listCases(limit: $limit, offset: $offset) {
        nodes {
          id
          inputFeatures
          prediction { pWin }
          recommendation { kind }
          createdAt
          createdBy
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
      console.error("[cases-page] gateway returned", resp.status);
      return EMPTY_CONNECTION;
    }

    const json = (await resp.json()) as {
      data?: { listCases?: CaseConnection };
      errors?: unknown[];
    };

    if (json.errors?.length) {
      console.error("[cases-page] GraphQL errors:", json.errors);
      return EMPTY_CONNECTION;
    }

    return json.data?.listCases ?? EMPTY_CONNECTION;
  } catch (err) {
    console.error("[cases-page] gateway fetch failed:", err);
    return EMPTY_CONNECTION;
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
  const connection = await fetchCases(offset);
  return (
    <CasesTable connection={connection} offset={offset} pageSize={PAGE_SIZE} />
  );
}
