import { describe, it, expect, vi, beforeEach } from "vitest";
import { LiveKitEngineAdapter, conformAnswerSdp } from "../livekit_adapter";
import { EventEmitter } from "events";
import { VideoQuality, Track, Room } from "livekit-client";

interface MockLocalParticipant {
  sid: string;
  identity: string;
  name: string;
  audioLevel: number;
  isSpeaking: boolean;
  isMicrophoneEnabled: boolean;
  isCameraEnabled: boolean;
  isScreenShareEnabled: boolean;
  trackPublications: Map<string, unknown>;
  publishTrack: ReturnType<typeof vi.fn>;
  unpublishTrack: ReturnType<typeof vi.fn>;
}

class MockLiveKitRoom extends EventEmitter {
  public state = "disconnected";
  public name = "test-lk-room";
  public localParticipant: MockLocalParticipant;
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
        (this.localParticipant as { trackPublications: Map<string, unknown> }).trackPublications.set(
          pub.trackSid,
          pub
        );
        return pub;
      }),
      unpublishTrack: vi.fn().mockResolvedValue(undefined),
    };
  }

  public async connect(_url?: string, _token?: string) {
    void _url;
    void _token;
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
    adapter = new LiveKitEngineAdapter(undefined, () => mockRoom as unknown as Room);
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

  it("hydrates existing remote participants and pre-subscribed tracks upon connection", async () => {
    const mockAttach = vi.fn().mockReturnValue({ remove: vi.fn(), parentElement: null });
    const mockPub = {
      trackSid: "audio-pub-1",
      source: "microphone",
      isMuted: false,
      isEnabled: true,
      isSubscribed: true,
      track: {
        sid: "audio-track-1",
        kind: "audio",
        attach: mockAttach,
        mediaStreamTrack: {},
      },
    };

    const mockExistingParticipant = {
      sid: "remote-p1",
      identity: "alice",
      name: "Alice",
      isSpeaking: false,
      audioLevel: 0,
      isMicrophoneEnabled: true,
      isCameraEnabled: true,
      isScreenShareEnabled: false,
      joinedAt: new Date(),
      trackPublications: new Map([["audio-pub-1", mockPub]]),
    };

    mockRoom.remoteParticipants.set("remote-p1", mockExistingParticipant);

    const joinedListener = vi.fn();
    const trackSubscribedListener = vi.fn();
    adapter.on("participantJoined", joinedListener);
    adapter.on("trackSubscribed", trackSubscribedListener);

    await adapter.connect({
      url: "ws://localhost:7880",
      token: "jwt-token-123",
      roomName: "test-room",
    });

    expect(joinedListener).toHaveBeenCalledWith(
      expect.objectContaining({
        id: "remote-p1",
        name: "Alice",
        isLocal: false,
      })
    );
    expect(trackSubscribedListener).toHaveBeenCalledWith(
      expect.objectContaining({
        id: "audio-pub-1",
        kind: "audio",
        source: "microphone",
        participantId: "remote-p1",
      }),
      expect.objectContaining({ id: "remote-p1" })
    );
    expect(mockAttach).toHaveBeenCalled();
  });

  it("attaches audio tracks on TrackSubscribed and detaches on TrackUnsubscribed/disconnect", async () => {
    await adapter.connect({
      url: "ws://localhost:7880",
      token: "jwt-token-123",
    });

    const mockRemove = vi.fn();
    const mockEl = { remove: mockRemove, parentElement: null };
    const mockAudioTrack = {
      sid: "audio-track-dynamic",
      kind: "audio",
      attach: vi.fn().mockReturnValue(mockEl),
      detach: vi.fn().mockReturnValue([mockEl]),
      mediaStreamTrack: {},
    };

    const mockPub = {
      trackSid: "pub-dyn",
      source: "microphone",
      isMuted: false,
      isEnabled: true,
    };

    const mockParticipant = {
      sid: "remote-p2",
      identity: "bob",
      name: "Bob",
      trackPublications: new Map(),
    };

    const trackSubListener = vi.fn();
    const trackUnsubListener = vi.fn();
    adapter.on("trackSubscribed", trackSubListener);
    adapter.on("trackUnsubscribed", trackUnsubListener);

    mockRoom.emit("trackSubscribed", mockAudioTrack, mockPub, mockParticipant);
    expect(mockAudioTrack.attach).toHaveBeenCalled();
    expect(trackSubListener).toHaveBeenCalledWith(
      expect.objectContaining({ id: "pub-dyn", kind: "audio" }),
      expect.objectContaining({ id: "remote-p2" })
    );

    mockRoom.emit("trackUnsubscribed", mockAudioTrack, mockPub, mockParticipant);
    expect(mockAudioTrack.detach).toHaveBeenCalled();
    expect(mockRemove).toHaveBeenCalled();
    expect(trackUnsubListener).toHaveBeenCalledWith("pub-dyn", "remote-p2");
  });

  it("disables simulcast when publishing screen share track", async () => {
    await adapter.connect({
      url: "ws://localhost:7880",
      token: "jwt-token-123",
    });

    const mockScreenTrack = {
      kind: "video",
      stop: vi.fn(),
    } as unknown as MediaStreamTrack;

    await adapter.publishTrack(mockScreenTrack, {
      source: "screen",
    });

    expect(mockRoom.localParticipant.publishTrack).toHaveBeenCalledWith(
      mockScreenTrack,
      expect.objectContaining({
        simulcast: false,
      })
    );
  });

  it("unpublishes track cleanly when given track sid or source name", async () => {
    await adapter.connect({
      url: "ws://localhost:7880",
      token: "jwt-token-123",
    });

    const mockTrack = { sid: "screen-track-1", stop: vi.fn() };
    const mockPublication = {
      trackSid: "pub-screen-1",
      track: mockTrack,
      source: Track.Source.ScreenShare,
    };
    mockRoom.localParticipant.trackPublications.set("pub-screen-1", mockPublication);

    // Unpublish by source name "screen"
    await adapter.unpublishTrack("screen");
    expect(mockRoom.localParticipant.unpublishTrack).toHaveBeenCalledWith(mockTrack, true);

    // Unpublish by track sid
    await adapter.unpublishTrack("pub-screen-1");
    expect(mockRoom.localParticipant.unpublishTrack).toHaveBeenCalledWith(mockTrack, true);
  });
});

describe("conformAnswerSdp", () => {
  it("coerces answer direction to inactive if offer was inactive (RFC 3264 Section 6.1)", () => {
    const offerSdp = [
      "v=0",
      "o=- 1 2 IN IP4 127.0.0.1",
      "s=-",
      "t=0 0",
      "m=video 9 UDP/TLS/RTP/SAVPF 96",
      "a=mid:3",
      "a=inactive",
    ].join("\r\n");

    const answerSdp = [
      "v=0",
      "o=- 1 2 IN IP4 127.0.0.1",
      "s=-",
      "t=0 0",
      "m=video 9 UDP/TLS/RTP/SAVPF 96",
      "a=mid:3",
      "a=recvonly",
    ].join("\r\n");

    const conformed = conformAnswerSdp(answerSdp, offerSdp);
    expect(conformed).toContain("a=inactive");
    expect(conformed).not.toContain("a=recvonly");
  });

  it("preserves valid offer-answer direction combinations", () => {
    const offerSdp = [
      "v=0",
      "o=- 1 2 IN IP4 127.0.0.1",
      "s=-",
      "t=0 0",
      "m=video 9 UDP/TLS/RTP/SAVPF 96",
      "a=mid:1",
      "a=sendrecv",
    ].join("\r\n");

    const answerSdp = [
      "v=0",
      "o=- 1 2 IN IP4 127.0.0.1",
      "s=-",
      "t=0 0",
      "m=video 9 UDP/TLS/RTP/SAVPF 96",
      "a=mid:1",
      "a=recvonly",
    ].join("\r\n");

    const conformed = conformAnswerSdp(answerSdp, offerSdp);
    expect(conformed).toContain("a=recvonly");
  });
});
