import { describe, it, expect, vi, beforeEach } from "vitest";
import { MediasoupOrtcEngineAdapter } from "../mediasoup_ortc_adapter";

describe("MediasoupOrtcEngineAdapter", () => {
  let adapter: MediasoupOrtcEngineAdapter;

  beforeEach(() => {
    adapter = new MediasoupOrtcEngineAdapter();
  });

  it("implements RtcEnginePort connection lifecycle", async () => {
    expect(adapter.getConnectionStatus()).toBe("disconnected");

    const statusSpy = vi.fn();
    adapter.on("connectionStatusChanged", statusSpy);

    await adapter.connect({
      url: "ws://localhost:8080/ws/ortc",
      token: "ortc-test-token",
      roomName: "ortc-room-1",
      participantName: "OrtcUser",
    });

    expect(adapter.getConnectionStatus()).toBe("connected");
    expect(adapter.getRoomId()).toBe("ortc-room-1");
    expect(adapter.getLocalParticipant()?.name).toBe("OrtcUser");
    expect(statusSpy).toHaveBeenCalledWith("connected");

    await adapter.disconnect();
    expect(adapter.getConnectionStatus()).toBe("disconnected");
    expect(statusSpy).toHaveBeenCalledWith("disconnected");
  });

  it("handles track publication, mute toggles and quality layer changes", async () => {
    await adapter.connect({
      url: "ws://localhost:8080/ws/ortc",
      token: "ortc-token",
    });

    const mockTrack = { kind: "video" } as unknown as MediaStreamTrack;
    const descriptor = await adapter.publishTrack(mockTrack, {
      source: "camera",
    });

    expect(descriptor.kind).toBe("video");
    expect(descriptor.source).toBe("camera");

    await adapter.setTrackMuted("camera", true);
    expect(adapter.isTrackMuted("camera")).toBe(true);

    await adapter.setVideoQuality(descriptor.id, "medium");
    await adapter.setTrackEnabled(descriptor.id, false);

    await adapter.unpublishTrack(descriptor.id);
  });
});
