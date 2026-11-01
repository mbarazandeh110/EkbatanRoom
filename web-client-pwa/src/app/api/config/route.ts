import { NextResponse } from "next/server";

export async function GET(request: Request) {
  const requestId =
    request.headers.get("x-request-id") ||
    `req-${Math.random().toString(36).substring(2, 9)}`;

  const config = {
    rtcProtocol: process.env.NEXT_PUBLIC_RTC_PROTOCOL || process.env.RTC_PROTOCOL || "livekit",
    livekitWsUrl: process.env.NEXT_PUBLIC_LIVEKIT_WS_URL || "ws://localhost:7880",
    ortcWsUrl: process.env.NEXT_PUBLIC_ORTC_WS_URL || "ws://localhost:8080/ws/ortc",
    metricsEnabled: process.env.OBS_METRICS_ENABLED !== "false",
    sampleRate: parseFloat(process.env.OBS_TRACE_SAMPLE_RATE || "0.1"),
  };

  return NextResponse.json(config, {
    status: 200,
    headers: {
      "x-request-id": requestId,
      "cache-control": "public, max-age=60",
    },
  });
}
