import { NextResponse } from "next/server";

export async function POST() {
  const response = new NextResponse(null, { status: 204 });
  // Clear the session cookie by expiring it immediately.
  response.cookies.set("jp_session", "", {
    httpOnly: true,
    sameSite: "lax",
    path: "/",
    maxAge: 0,
  });
  return response;
}
