import { describe, it, expect, beforeEach } from "vitest";
import { useRoomStore } from "../use_room_store";
import {
  useParticipantsStore,
  useParticipant,
  useParticipantTrack,
} from "../use_participants_store";
import {
  useTelemetryStore,
  useParticipantAudioLevel,
  useIsParticipantSpeaking,
} from "../use_telemetry_store";
import { ParticipantModel } from "../../domain/models/participant";
import { TrackDescriptor } from "../../domain/models/media";

describe("Granular Zustand Stores", () => {
  beforeEach(() => {
    useRoomStore.getState().resetRoom();
    useParticipantsStore.getState().clearParticipants();
    useTelemetryStore.getState().clearTelemetry();
  });

  describe("useRoomStore", () => {
    it("handles room status, layout, and participant ID tracking", () => {
      useRoomStore.getState().setRoomInfo("room-123", "Demo Room");
      useRoomStore.getState().setConnectionStatus("connected");
      useRoomStore.getState().setLayoutMode("speaker");
      useRoomStore.getState().addRemoteParticipantId("user-1");
      useRoomStore.getState().addRemoteParticipantId("user-2");

      const state = useRoomStore.getState();
      expect(state.roomId).toBe("room-123");
      expect(state.roomName).toBe("Demo Room");
      expect(state.status).toBe("connected");
      expect(state.layoutMode).toBe("speaker");
      expect(state.remoteParticipantIds).toEqual(["user-1", "user-2"]);

      useRoomStore.getState().removeRemoteParticipantId("user-1");
      expect(useRoomStore.getState().remoteParticipantIds).toEqual(["user-2"]);
    });
  });

  describe("useParticipantsStore", () => {
    it("upserts and removes participants and tracks cleanly", () => {
      const participant: ParticipantModel = {
        id: "p-1",
        identity: "john",
        name: "John Doe",
        isLocal: false,
        role: "speaker",
        isSpeaking: false,
        audioLevel: 0,
        connectionQuality: "good",
        isMicMuted: false,
        isCameraMuted: false,
        isScreenSharing: false,
        joinedAt: 1000,
      };

      useParticipantsStore.getState().upsertParticipant(participant);
      expect(useParticipantsStore.getState().participants["p-1"]).toEqual(participant);

      const track: TrackDescriptor = {
        id: "t-1",
        kind: "video",
        source: "camera",
        participantId: "p-1",
        isMuted: false,
        isEnabled: true,
      };

      useParticipantsStore.getState().upsertTrack(track);
      expect(useParticipantsStore.getState().tracks["t-1"]).toEqual(track);
      expect(useParticipantsStore.getState().participantTracks["p-1"]).toContain("t-1");

      // Mute track
      useParticipantsStore.getState().setTrackMuted("t-1", true);
      expect(useParticipantsStore.getState().tracks["t-1"].isMuted).toBe(true);
      expect(useParticipantsStore.getState().participants["p-1"].isCameraMuted).toBe(true);

      // Remove participant also removes participant's tracks
      useParticipantsStore.getState().removeParticipant("p-1");
      expect(useParticipantsStore.getState().participants["p-1"]).toBeUndefined();
      expect(useParticipantsStore.getState().tracks["t-1"]).toBeUndefined();
      expect(useParticipantsStore.getState().participantTracks["p-1"]).toBeUndefined();
    });
  });

  describe("useTelemetryStore", () => {
    it("updates high-frequency audio levels and active speakers without mutating participants store", () => {
      useTelemetryStore.getState().setAudioLevels({ "p-1": 0.85, "p-2": 0.12 });
      useTelemetryStore.getState().setActiveSpeakers(["p-1"]);

      expect(useTelemetryStore.getState().audioLevels["p-1"]).toBe(0.85);
      expect(useTelemetryStore.getState().activeSpeakerIds).toEqual(["p-1"]);

      // Verify that participants store remains untouched (Zero Re-render Lag guarantee)
      expect(Object.keys(useParticipantsStore.getState().participants)).toHaveLength(0);
    });
  });
});
