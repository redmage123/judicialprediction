/**
 * GET /api/case/:id/memo.pdf
 *
 * Returns a single-page Letter PDF evaluation memo for the requested case.
 *
 * STRATEGY: Strategy B (React-PDF, @react-pdf/renderer).
 * See web/lib/memo/case-memo.tsx for the full strategy rationale.
 *
 * Auth: reads the `jp_session` httpOnly cookie (same as the /case/[id] RSC page).
 *
 * Errors:
 *   404 — case not found or belongs to a different tenant.
 *   401 — no session cookie present.
 *   502 — api-gateway unreachable.
 *
 * Sprint-5 follow-ups:
 *   - Strategy A: swap render step for a Playwright headless pass over ?print=1
 *     if pixel-perfect parity with the live results view is required.
 *   - Stream the PDF instead of buffering (for very large future multi-page memos).
 */

import React from "react";
import { cookies } from "next/headers";
import { pdf } from "@react-pdf/renderer";
import CaseMemo from "@/lib/memo/case-memo";
import type { CaseResult } from "@/lib/queries/predict";

const GATEWAY_URL =
  process.env.GATEWAY_INTERNAL_URL ?? "http://localhost:4000";

// ---------------------------------------------------------------------------
// GraphQL query — identical field set to the /case/[id] page.tsx RSC fetch
// ---------------------------------------------------------------------------

const GET_CASE_QUERY = `
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

// ---------------------------------------------------------------------------
// Case fetcher — mirroring the pattern in /case/[id]/page.tsx
// ---------------------------------------------------------------------------

async function fetchCase(
  id: string,
  token: string | undefined
): Promise<CaseResult | null> {
  try {
    const resp = await fetch(`${GATEWAY_URL}/graphql`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify({ query: GET_CASE_QUERY, variables: { id } }),
      cache: "no-store",
    });

    if (!resp.ok) return null;

    const json = (await resp.json()) as {
      data?: { case?: CaseResult | null };
      errors?: unknown[];
    };

    if (json.errors?.length) {
      console.error("[memo.pdf] GraphQL errors:", json.errors);
      return null;
    }

    return json.data?.case ?? null;
  } catch (err) {
    console.error("[memo.pdf] gateway fetch failed:", err);
    return null;
  }
}

// ---------------------------------------------------------------------------
// Route handler
// ---------------------------------------------------------------------------

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ id: string }> }
): Promise<Response> {
  const { id } = await params;

  const cookieStore = await cookies();
  const token = cookieStore.get("jp_session")?.value;

  if (!token) {
    return Response.json({ error: "Unauthorized" }, { status: 401 });
  }

  const caseResult = await fetchCase(id, token);

  if (!caseResult) {
    return Response.json(
      { error: `Case ${id} not found or not accessible` },
      { status: 404 }
    );
  }

  // Render the React-PDF document to a Node.js Buffer.
  // v4 toBuffer() returns a ReadableStream; collect into a Buffer for Response.
  const stream = await pdf(<CaseMemo caseResult={caseResult} />).toBuffer();
  const chunks: Buffer[] = [];
  for await (const chunk of stream as unknown as AsyncIterable<Buffer | Uint8Array>) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  const buffer = Buffer.concat(chunks);

  // Use only the first 8 characters of the UUID for the filename to keep it
  // readable; the full ID is embedded in the PDF footer.
  const shortId = id.slice(0, 8);

  return new Response(buffer, {
    status: 200,
    headers: {
      "content-type": "application/pdf",
      "content-disposition": `attachment; filename="case-${shortId}.pdf"`,
      // Prevent caching — the operator may regenerate the PDF after edits.
      "cache-control": "no-store",
    },
  });
}
