import { describe, it, expect } from "vitest";
import { POST } from "../route";
import { NextRequest } from "next/server";

describe("POST /api/recording", () => {
  it("rejects request with missing or invalid roomId", async () => {
    const req = new NextRequest("http://localhost:3000/api/recording", {
      method: "POST",
      body: JSON.stringify({ action: "start" }),
    });

    const res = await POST(req);
    expect(res.status).toBe(400);
    const data = await res.json();
    expect(data.error).toContain("roomId");
  });

  it("handles valid start recording request", async () => {
    const req = new NextRequest("http://localhost:3000/api/recording", {
      method: "POST",
      body: JSON.stringify({ roomId: "test-room-101", action: "start" }),
    });

    const res = await POST(req);
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data.success).toBe(true);
    expect(data.status).toBe("started");
    expect(data.recordingId).toBeDefined();
    expect(data.s3Destination).toContain("test-room-101");
  });

  it("handles valid stop recording request", async () => {
    const req = new NextRequest("http://localhost:3000/api/recording", {
      method: "POST",
      body: JSON.stringify({ roomId: "test-room-101", action: "stop" }),
    });

    const res = await POST(req);
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data.success).toBe(true);
    expect(data.status).toBe("stopped");
  });
});
