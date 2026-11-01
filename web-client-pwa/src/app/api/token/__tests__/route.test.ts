/**
 * @vitest-environment node
 */
import { describe, it, expect } from "vitest";
import { POST } from "../route";
import { jwtVerify } from "jose";

describe("/api/token Route Handler", () => {
  it("issues a signed LiveKit JWT token for room and identity", async () => {
    const req = new Request("http://localhost:3000/api/token", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        room: "engineering-sync",
        identity: "user-42",
        name: "Alice",
      }),
    });

    const res = await POST(req);
    expect(res.status).toBe(200);

    const data = await res.json();
    expect(data.token).toBeDefined();
    expect(data.url).toBe("ws://localhost:7880");

    // Verify token claims
    const secret = new TextEncoder().encode(process.env.LIVEKIT_API_SECRET || "secret");
    const { payload } = await jwtVerify(data.token, secret);

    expect(payload.sub).toBe("user-42");
    expect(payload.iss).toBe("devkey");
    expect((payload as any).video?.room).toBe("engineering-sync");
    expect((payload as any).video?.roomJoin).toBe(true);
  });

  it("returns 400 when required fields are missing", async () => {
    const req = new Request("http://localhost:3000/api/token", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        room: "engineering-sync",
      }),
    });

    const res = await POST(req);
    expect(res.status).toBe(400);

    const data = await res.json();
    expect(data.error).toContain("Missing required fields");
  });
});
