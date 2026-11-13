import { NextRequest, NextResponse } from "next/server";

export async function POST(req: NextRequest) {
  try {
    const body = await req.json();
    const { roomId, action } = body || {};

    if (!roomId || typeof roomId !== "string") {
      return NextResponse.json(
        { error: "Missing or invalid roomId parameter" },
        { status: 400 }
      );
    }

    if (action !== "start" && action !== "stop") {
      return NextResponse.json(
        { error: "Invalid action. Expected 'start' or 'stop'" },
        { status: 400 }
      );
    }

    if (action === "start") {
      const recordingId = `rec-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
      return NextResponse.json({
        success: true,
        recordingId,
        status: "started",
        s3Destination: `s3://ekbatan-recordings/${roomId}/${recordingId}.mp4`,
      });
    } else {
      return NextResponse.json({
        success: true,
        status: "stopped",
      });
    }
  } catch (error) {
    return NextResponse.json(
      { error: "Failed to process cloud recording request", details: String(error) },
      { status: 500 }
    );
  }
}
