// S4.4 (JP-58): converted from pass-through to a true RSC server component.
//
// Fetches the case from api-gateway server-side (reads jp_session cookie,
// attaches Authorization header — same pattern as the BFF proxy at /api/graphql).
// Passes the Case data as a prop to ResultsView, which is now a pure
// presentational component with no sessionStorage dependency.
//
// If the case is null (not found, wrong tenant, or gateway error) the
// empty-state EmptyState component is rendered with a CTA back to /case/new.

import type { Metadata } from "next";
import { cookies } from "next/headers";
import { ResultsView } from "./results-view";
import type { CaseResult } from "@/lib/queries/predict";

export const dynamic = "force-dynamic";

const GATEWAY_URL =
  process.env.GATEWAY_INTERNAL_URL ?? "http://localhost:4000";

type Props = {
  params: Promise<{ id: string }>;
};

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { id } = await params;
  const prefix = id.slice(0, 8);
  return {
    title: `Case ${prefix} — JudicialPredict`,
  };
}

/** Fetch a single case from the api-gateway using the GetCase query. */
async function fetchCase(id: string): Promise<CaseResult | null> {
  const cookieStore = await cookies();
  const token = cookieStore.get("jp_session")?.value;

  const query = `
    query GetCase($id: ID!) {
      case(id: $id) {
        id
        tenantId
        inputFeatures
        prediction {
          pWin
          ciLower
          ciUpper
          coverage
          modelVersion
          predictedAtUnix
        }
        recommendation {
          kind
          rationaleBullets
          expectedValueTry
          expectedValueSettle
        }
        createdBy
        createdAt
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
      body: JSON.stringify({ query, variables: { id } }),
      cache: "no-store",
    });

    if (!resp.ok) return null;

    const json = (await resp.json()) as {
      data?: { case?: CaseResult | null };
      errors?: unknown[];
    };

    if (json.errors?.length) {
      console.error("[case-page] GraphQL errors:", json.errors);
      return null;
    }

    return json.data?.case ?? null;
  } catch (err) {
    console.error("[case-page] gateway fetch failed:", err);
    return null;
  }
}

export default async function CasePage({ params }: Props) {
  const { id } = await params;
  const caseResult = await fetchCase(id);
  // ResultsView is a presentational component — no sessionStorage, no client state.
  return <ResultsView caseResult={caseResult} />;
}
