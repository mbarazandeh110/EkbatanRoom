import { describe, it, expect, vi, beforeEach } from "vitest";
import { LiveKitEngineAdapter } from "../livekit_adapter";
import { EventEmitter } from "events";
import { VideoQuality } from "livekit-client";

class MockLiveKitRoom extends EventEmitter {
  public state = "disconnected";
  public name = "test-lk-room";
  public localParticipant: any = null;
  public remoteParticipants = new Map();

  constructor() {
    super();
    this.localParticipant = {
      sid: "local-sid-1",
      identity: "local-user",
      name: "Local User",
      audioLevel: 0,
      isSpeaking: false,
      isMicrophoneEnabled: true,
      isCameraEnabled: true,
      isScreenShareEnabled: false,
      trackPublications: new Map(),
      publishTrack: vi.fn().mockImplementation(async (track, options) => {
        const pub = {
          trackSid: `pub-${track.kind}`,
          track: { sid: `track-${track.kind}` },
          isMuted: false,
          source: options.source,
          mute: vi.fn().mockImplementation(async () => {
            pub.isMuted = true;
          }),
          unmute: vi.fn().mockImplementation(async () => {
            pub.isMuted = false;
          }),
        };
        this.localParticipant.trackPublications.set(pub.trackSid, pub);
        return pub;
      }),
      unpublishTrack: vi.fn().mockResolvedValue(undefined),
    };
  }

  public async connect(url: string, token: string) {
    this.state = "connected";
    this.emit("connected");
    this.emit("connectionStateChanged", "connected");
  }

  public async disconnect() {
    this.state = "disconnected";
    this.emit("disconnected");
    this.emit("connectionStateChanged", "disconnected");
  }
}

describe("LiveKitEngineAdapter", () => {
  let mockRoom: MockLiveKitRoom;
  let adapter: LiveKitEngineAdapter;

  beforeEach(() => {
    mockRoom = new MockLiveKitRoom();
    adapter = new LiveKitEngineAdapter(undefined, () => mockRoom as any);
  });

  it("connects and disconnects mapping room state", async () => {
    expect(adapter.getConnectionStatus()).toBe("disconnected");

    await adapter.connect({
      url: "ws://localhost:7880",
      token: "jwt-token-123",
      roomName: "test-room",
    });

    expect(adapter.getConnectionStatus()).toBe("connected");
    expect(adapter.getRoomId()).toBe("test-lk-room");
    expect(adapter.getLocalParticipant()?.id).toBe("local-sid-1");

    await adapter.disconnect();
    expect(adapter.getConnectionStatus()).toBe("disconnected");
  });

  it("publishes local track with simulcast and maps to TrackDescriptor", async () => {
    await adapter.connect({
      url: "ws://localhost:7880",
      token: "jwt-token-123",
    });

    const mockTrack = {
      kind: "video",
      stop: vi.fn(),
    } as unknown as MediaStreamTrack;

    const descriptor = await adapter.publishTrack(mockTrack, {
      source: "camera",
      simulcast: true,
    });

    expect(descriptor.kind).toBe("video");
    expect(descriptor.source).toBe("camera");
    expect(mockRoom.localParticipant.publishTrack).toHaveBeenCalledWith(
      mockTrack,
      expect.objectContaining({ simulcast: true })
    );
  });

  it("mutes and unmutes published tracks", async () => {
    await adapter.connect({
      url: "ws://localhost:7880",
      token: "jwt-token-123",
    });

    const mockTrack = { kind: "audio" } as unknown as MediaStreamTrack;
    await adapter.publishTrack(mockTrack, { source: "microphone" });

    expect(adapter.isTrackMuted("microphone")).toBe(false);

    await adapter.setTrackMuted("microphone", true);
    expect(adapter.isTrackMuted("microphone")).toBe(true);

    await adapter.setTrackMuted("microphone", false);
    expect(adapter.isTrackMuted("microphone")).toBe(false);
  });

  it("maps remote track quality selection to LiveKit VideoQuality enums", async () => {
    await adapter.connect({
      url: "ws://localhost:7880",
      token: "jwt-token-123",
    });

    const mockSetVideoQuality = vi.fn();
    const mockSetEnabled = vi.fn();

    const mockPub = {
      setVideoQuality: mockSetVideoQuality,
      setEnabled: mockSetEnabled,
    };
    Object.setPrototypeOf(mockPub, (await import("livekit-client")).RemoteTrackPublication.prototype);

    const mockRemoteParticipant = {
      getTrackPublication: vi.fn().mockReturnValue(mockPub),
    };
    mockRoom.remoteParticipants.set("remote-peer-1", mockRemoteParticipant);

    await adapter.setVideoQuality("remote-track-1", "low");
    expect(mockSetVideoQuality).toHaveBeenCalledWith(VideoQuality.LOW);

    await adapter.setVideoQuality("remote-track-1", "medium");
    expect(mockSetVideoQuality).toHaveBeenCalledWith(VideoQuality.MEDIUM);

    await adapter.setVideoQuality("remote-track-1", "high");
    expect(mockSetVideoQuality).toHaveBeenCalledWith(VideoQuality.HIGH);

    await adapter.setTrackEnabled("remote-track-1", false);
    expect(mockSetEnabled).toHaveBeenCalledWith(false);
  });

  it("maps active speakers event", async () => {
    await adapter.connect({
      url: "ws://localhost:7880",
      token: "jwt-token-123",
    });

    const speakerListener = vi.fn();
    adapter.on("activeSpeakersChanged", speakerListener);

    mockRoom.emit("activeSpeakersChanged", [
      { sid: "speaker-1", identity: "spk1", audioLevel: 0.9 },
    ]);

    expect(speakerListener).toHaveBeenCalledWith([
      { participantId: "speaker-1", audioLevel: 0.9 },
    ]);
  });
});
