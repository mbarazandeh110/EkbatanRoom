import { NextResponse } from "next/server";

export async function GET(request: Request) {
  const requestId =
    request.headers.get("x-request-id") ||
    `req-${Math.random().toString(36).substring(2, 9)}`;

  return NextResponse.json(
    {
      status: "healthy",
      service: "web-client-pwa",
      timestamp: new Date().toISOString(),
      uptime: process.uptime(),
    },
    {
      status: 200,
      headers: {
        "x-request-id": requestId,
        "cache-control": "no-store, max-age=0",
      },
    }
  );
}
