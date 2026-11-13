import { describe, it, expect, beforeEach } from "vitest";
import { RoomSessionController } from "../room_session_controller";
import { MockRtcEngineAdapter } from "../../adapters/mock/mock_rtc_engine_adapter";
import { useRoomStore } from "../../store/use_room_store";
import { useParticipantsStore } from "../../store/use_participants_store";
import { useTelemetryStore } from "../../store/use_telemetry_store";
import { ParticipantModel } from "../../domain/models/participant";

describe("RoomSessionController", () => {
  let engine: MockRtcEngineAdapter;
  let controller: RoomSessionController;

  beforeEach(() => {
    useRoomStore.getState().resetRoom();
    useParticipantsStore.getState().clearParticipants();
    useTelemetryStore.getState().clearTelemetry();

    engine = new MockRtcEngineAdapter();
    controller = new RoomSessionController(engine);
  });

  it("should join room and synchronize room status and local participant in stores", async () => {
    expect(useRoomStore.getState().status).toBe("disconnected");

    await controller.joinRoom({
      url: "ws://localhost:7880",
      token: "mock-jwt-token",
      roomName: "engineering-sync",
      participantName: "Alice",
    });

    const roomState = useRoomStore.getState();
    expect(roomState.status).toBe("connected");
    expect(roomState.roomName).toBe("engineering-sync");
    expect(roomState.localParticipantId).toBe("local-user-1");

    const localP = useParticipantsStore.getState().participants["local-user-1"];
    expect(localP).toBeDefined();
    expect(localP?.name).toBe("Alice");
    expect(localP?.isLocal).toBe(true);
  });

  it("should synchronize remote participant join and leave", async () => {
    await controller.joinRoom({
      url: "ws://localhost:7880",
      token: "mock-jwt",
      roomName: "team-standup",
    });

    const remoteParticipant: ParticipantModel = {
      id: "remote-user-42",
      identity: "bob",
      name: "Bob",
      isLocal: false,
      role: "speaker",
      isSpeaking: false,
      audioLevel: 0,
      connectionQuality: "good",
      isMicMuted: false,
      isCameraMuted: false,
      isScreenSharing: false,
      joinedAt: Date.now(),
    };

    engine.simulateRemoteParticipantJoin(remoteParticipant);

    expect(useRoomStore.getState().remoteParticipantIds).toContain("remote-user-42");
    expect(useParticipantsStore.getState().participants["remote-user-42"]).toBeDefined();

    engine.simulateRemoteParticipantLeave("remote-user-42");

    expect(useRoomStore.getState().remoteParticipantIds).not.toContain("remote-user-42");
    expect(useParticipantsStore.getState().participants["remote-user-42"]).toBeUndefined();
  });

  it("should synchronize remote track subscriptions", async () => {
    await controller.joinRoom({
      url: "ws://localhost:7880",
      token: "mock-jwt",
      roomName: "team-standup",
    });

    const remoteParticipant: ParticipantModel = {
      id: "remote-user-99",
      identity: "carol",
      name: "Carol",
      isLocal: false,
      role: "speaker",
      isSpeaking: false,
      audioLevel: 0,
      connectionQuality: "excellent",
      isMicMuted: false,
      isCameraMuted: false,
      isScreenSharing: false,
      joinedAt: Date.now(),
    };

    engine.simulateTrackSubscribed(
      {
        id: "track-video-99",
        kind: "video",
        source: "camera",
        participantId: "remote-user-99",
        isMuted: false,
        isEnabled: true,
      },
      remoteParticipant
    );

    const track = useParticipantsStore.getState().tracks["track-video-99"];
    expect(track).toBeDefined();
    expect(track.source).toBe("camera");
    expect(useRoomStore.getState().remoteParticipantIds).toContain("remote-user-99");
  });

  it("should toggle and update track mute state", async () => {
    await controller.joinRoom({
      url: "ws://localhost:7880",
      token: "mock-jwt",
      roomName: "audio-check",
    });

    expect(engine.isTrackMuted("microphone")).toBe(false);

    const nextMuted = await controller.toggleTrackMute("microphone");
    expect(nextMuted).toBe(true);
    expect(engine.isTrackMuted("microphone")).toBe(true);

    const localP = useParticipantsStore.getState().participants["local-user-1"];
    expect(localP?.isMicMuted).toBe(true);
  });

  it("should reset stores on leaveRoom", async () => {
    await controller.joinRoom({
      url: "ws://localhost:7880",
      token: "mock-jwt",
      roomName: "cleanup-test",
    });

    await controller.leaveRoom();

    expect(useRoomStore.getState().status).toBe("disconnected");
    expect(useRoomStore.getState().roomId).toBeNull();
    expect(Object.keys(useParticipantsStore.getState().participants)).toHaveLength(0);
    expect(useTelemetryStore.getState().activeSpeakerIds).toHaveLength(0);
  });
});
