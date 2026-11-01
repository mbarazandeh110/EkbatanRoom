import {
  RtcEnginePort,
  ConnectOptions,
  PublishTrackOptions,
} from "../ports/rtc_engine_port";
import { TelemetryPort } from "../ports/telemetry_port";
import { TrackSource, TrackDescriptor } from "../domain/models/media";
import { useRoomStore } from "../store/use_room_store";
import { useParticipantsStore } from "../store/use_participants_store";
import { useTelemetryStore } from "../store/use_telemetry_store";

export class RoomSessionController {
  private isSubscribedToEngineEvents: boolean = false;

  constructor(
    private engine: RtcEnginePort,
    private telemetry?: TelemetryPort
  ) {
    this.bindEngineEvents();
  }

  private bindEngineEvents(): void {
    if (this.isSubscribedToEngineEvents) return;
    this.isSubscribedToEngineEvents = true;

    this.engine.on("connectionStatusChanged", (status, error) => {
      useRoomStore.getState().setConnectionStatus(status, error?.message || null);
      this.telemetry?.log(
        status === "failed" ? "error" : "info",
        "connection_status_changed",
        { status, error: error?.message }
      );
    });

    this.engine.on("participantJoined", (participant) => {
      useParticipantsStore.getState().upsertParticipant(participant);
      if (!participant.isLocal) {
        useRoomStore.getState().addRemoteParticipantId(participant.id);
      }
      this.telemetry?.log("info", "participant_joined", {
        participantId: participant.id,
        isLocal: participant.isLocal,
      });
    });

    this.engine.on("participantLeft", (participantId) => {
      useParticipantsStore.getState().removeParticipant(participantId);
      useRoomStore.getState().removeRemoteParticipantId(participantId);
      this.telemetry?.log("info", "participant_left", { participantId });
    });

    this.engine.on("trackSubscribed", (track, participant) => {
      useParticipantsStore.getState().upsertParticipant(participant);
      useParticipantsStore.getState().upsertTrack(track);
      this.telemetry?.log("debug", "track_subscribed", {
        trackId: track.id,
        source: track.source,
        participantId: participant.id,
      });
    });

    this.engine.on("trackUnsubscribed", (trackId, participantId) => {
      useParticipantsStore.getState().removeTrack(trackId, participantId);
      this.telemetry?.log("debug", "track_unsubscribed", {
        trackId,
        participantId,
      });
    });

    this.engine.on("trackMutedChanged", (trackId, participantId, isMuted) => {
      useParticipantsStore.getState().setTrackMuted(trackId, isMuted);
      this.telemetry?.log("debug", "track_muted_changed", {
        trackId,
        participantId,
        isMuted,
      });
    });

    this.engine.on("activeSpeakersChanged", (speakers) => {
      const activeIds = speakers.map((s) => s.participantId);
      useTelemetryStore.getState().setActiveSpeakers(activeIds);
    });

    this.engine.on("audioLevelsChanged", (levels) => {
      useTelemetryStore.getState().setAudioLevels(levels);
    });

    this.engine.on("connectionQualityChanged", (participantId, quality) => {
      useParticipantsStore.getState().setParticipantQuality(participantId, quality);
    });
  }

  public async joinRoom(options: ConnectOptions): Promise<void> {
    const span = this.telemetry?.startSpan("join_room", {
      roomName: options.roomName,
    });
    try {
      useRoomStore.getState().setConnectionStatus("connecting");
      await this.engine.connect(options);

      const roomId = this.engine.getRoomId();
      useRoomStore.getState().setRoomInfo(roomId, options.roomName);
      useRoomStore.getState().setConnectionStatus("connected");

      const localParticipant = this.engine.getLocalParticipant();
      if (localParticipant) {
        useRoomStore.getState().setLocalParticipantId(localParticipant.id);
        useParticipantsStore.getState().upsertParticipant(localParticipant);
      }

      span?.end({ roomId, status: "connected" });
    } catch (error) {
      useRoomStore.getState().setConnectionStatus("failed", String(error));
      span?.recordException(error instanceof Error ? error : new Error(String(error)));
      throw error;
    }
  }

  public async leaveRoom(): Promise<void> {
    const span = this.telemetry?.startSpan("leave_room");
    try {
      await this.engine.disconnect();
      useRoomStore.getState().resetRoom();
      useParticipantsStore.getState().clearParticipants();
      useTelemetryStore.getState().clearTelemetry();
      span?.end();
    } catch (error) {
      span?.recordException(error instanceof Error ? error : new Error(String(error)));
      throw error;
    }
  }

  public async publishLocalTrack(
    track: MediaStreamTrack,
    options: PublishTrackOptions
  ): Promise<TrackDescriptor> {
    const published = await this.engine.publishTrack(track, options);
    useParticipantsStore.getState().upsertTrack(published);
    return published;
  }

  public async unpublishLocalTrack(trackId: string): Promise<void> {
    await this.engine.unpublishTrack(trackId);
    useParticipantsStore.getState().removeTrack(trackId);
  }

  public async setTrackMuted(source: TrackSource, muted: boolean): Promise<void> {
    await this.engine.setTrackMuted(source, muted);
    const localId = useRoomStore.getState().localParticipantId;
    if (localId) {
      useParticipantsStore.getState().setParticipantMuteState(localId, source, muted);
    }
  }

  public async toggleTrackMute(source: TrackSource): Promise<boolean> {
    const isCurrentlyMuted = this.engine.isTrackMuted(source);
    const nextMuted = !isCurrentlyMuted;
    await this.setTrackMuted(source, nextMuted);
    return nextMuted;
  }
}
