/**
 * POST /api/graphql — BFF (Backend-for-Frontend) GraphQL proxy.
 *
 * All client-side Apollo requests go here instead of directly to api-gateway.
 * This proxy reads the httpOnly jp_session cookie server-side and attaches
 * an Authorization: Bearer header, so the JWT never needs to be readable
 * by browser JS.
 */
import { cookies } from "next/headers";
import { NextRequest, NextResponse } from "next/server";

const GATEWAY_GRAPHQL =
  process.env.GATEWAY_INTERNAL_URL
    ? `${process.env.GATEWAY_INTERNAL_URL}/graphql`
    : "http://localhost:4000/graphql";

export async function POST(request: NextRequest) {
  const cookieStore = await cookies();
  const token = cookieStore.get("jp_session")?.value;

  const upstreamHeaders: Record<string, string> = {
    "content-type": "application/json",
  };
  if (token) {
    upstreamHeaders["authorization"] = `Bearer ${token}`;
  }

  let body: string;
  try {
    body = await request.text();
  } catch {
    return NextResponse.json(
      { errors: [{ message: "Failed to read request body" }] },
      { status: 400 }
    );
  }

  try {
    const upstream = await fetch(GATEWAY_GRAPHQL, {
      method: "POST",
      headers: upstreamHeaders,
      body,
      // Do not cache GraphQL responses.
      cache: "no-store",
    });

    const text = await upstream.text();
    return new NextResponse(text, {
      status: upstream.status,
      headers: { "content-type": "application/json" },
    });
  } catch (err) {
    console.error("[graphql-proxy] upstream fetch failed:", err);
    return NextResponse.json(
      { errors: [{ message: "api-gateway unreachable" }] },
      { status: 502 }
    );
  }
}
