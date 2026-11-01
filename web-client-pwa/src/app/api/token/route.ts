import { NextResponse } from "next/server";
import { SignJWT } from "jose";

export async function POST(request: Request) {
  const requestId =
    request.headers.get("x-request-id") ||
    `req-${Math.random().toString(36).substring(2, 9)}`;
  const traceparent = request.headers.get("traceparent") || undefined;

  try {
    const body = await request.json();
    const { room, identity, name } = body;

    if (!room || !identity) {
      return NextResponse.json(
        { error: "Missing required fields: room, identity" },
        { status: 400, headers: { "x-request-id": requestId } }
      );
    }

    const apiKey = process.env.LIVEKIT_API_KEY || "devkey";
    const apiSecret = process.env.LIVEKIT_API_SECRET || "secret";
    const wsUrl =
      process.env.NEXT_PUBLIC_LIVEKIT_WS_URL ||
      process.env.LIVEKIT_WS_URL ||
      "ws://localhost:7880";

    const now = Math.floor(Date.now() / 1000);
    const ttl = 600; // 10 minutes default

    const payload = {
      sub: identity,
      iss: apiKey,
      nbf: now - 5,
      exp: now + ttl,
      name: name || identity,
      video: {
        room: room,
        roomJoin: true,
        canPublish: true,
        canSubscribe: true,
        canPublishData: true,
      },
    };

    const secretKey = new TextEncoder().encode(apiSecret);
    const token = await new SignJWT(payload)
      .setProtectedHeader({ alg: "HS256", typ: "JWT" })
      .setIssuedAt(now)
      .setExpirationTime(now + ttl)
      .sign(secretKey);

    // Structured server log without sensitive raw JWT token
    const logEntry = {
      timestamp: new Date().toISOString(),
      level: "info",
      event: "token_issued",
      requestId,
      traceId: traceparent ? traceparent.split("-")[1] : undefined,
      room,
      identity,
      iss: apiKey,
      token: "[REDACTED]",
    };
    console.log(JSON.stringify(logEntry));

    return NextResponse.json(
      {
        token,
        url: wsUrl,
      },
      {
        status: 200,
        headers: {
          "x-request-id": requestId,
          "cache-control": "no-store, max-age=0",
        },
      }
    );
  } catch (err: any) {
    const errorEntry = {
      timestamp: new Date().toISOString(),
      level: "error",
      event: "token_issuance_failed",
      requestId,
      error: err?.message || String(err),
    };
    console.error(JSON.stringify(errorEntry));

    return NextResponse.json(
      { error: "Failed to issue token" },
      { status: 500, headers: { "x-request-id": requestId } }
    );
  }
}
