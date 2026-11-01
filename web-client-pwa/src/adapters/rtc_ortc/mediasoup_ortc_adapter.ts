import {
  RtcEnginePort,
  ConnectOptions,
  PublishTrackOptions,
  RtcEngineEvents,
} from "../../ports/rtc_engine_port";
import {
  ConnectionStatus,
  ParticipantModel,
  TrackDescriptor,
  TrackSource,
  TrackKind,
  VideoQualityLayer,
} from "../../ports/rtc_engine_port";
import { TelemetryPort } from "../../ports/telemetry_port";

export class MediasoupOrtcEngineAdapter implements RtcEnginePort {
  private status: ConnectionStatus = "disconnected";
  private roomId: string = "";
  private localParticipant: ParticipantModel | null = null;
  private publishedTracks: Map<string, TrackDescriptor> = new Map();
  private mutedSources: Map<TrackSource, boolean> = new Map();
  private qualityLayers: Map<string, VideoQualityLayer> = new Map();
  private enabledTracks: Map<string, boolean> = new Map();

  private eventListeners: {
    [K in keyof RtcEngineEvents]?: Array<RtcEngineEvents[K]>;
  } = {};

  constructor(private telemetry?: TelemetryPort) {}

  public on<E extends keyof RtcEngineEvents>(
    event: E,
    handler: RtcEngineEvents[E]
  ): void {
    if (!this.eventListeners[event]) {
      this.eventListeners[event] = [];
    }
    this.eventListeners[event]!.push(handler);
  }

  public off<E extends keyof RtcEngineEvents>(
    event: E,
    handler: RtcEngineEvents[E]
  ): void {
    const list = this.eventListeners[event];
    if (list) {
      this.eventListeners[event] = list.filter((h) => h !== handler) as any;
    }
  }

  public emit<E extends keyof RtcEngineEvents>(
    event: E,
    ...args: Parameters<RtcEngineEvents[E]>
  ): void {
    const handlers = this.eventListeners[event];
    if (handlers) {
      handlers.forEach((h: any) => h(...args));
    }
  }

  public async connect(options: ConnectOptions): Promise<void> {
    this.status = "connecting";
    this.emit("connectionStatusChanged", "connecting");
    this.telemetry?.log("info", "ortc_connecting", {
      url: options.url,
      roomName: options.roomName,
    });

    this.roomId = options.roomName || "ortc-room";
    this.localParticipant = {
      id: "ortc-local-peer",
      identity: options.participantName || "ortc-user",
      name: options.participantName || "ORTC User",
      isLocal: true,
      role: "speaker",
      isSpeaking: false,
      audioLevel: 0,
      connectionQuality: "good",
      isMicMuted: false,
      isCameraMuted: false,
      isScreenSharing: false,
      joinedAt: Date.now(),
    };

    this.status = "connected";
    this.emit("connectionStatusChanged", "connected");
    this.emit("participantJoined", this.localParticipant);
  }

  public async disconnect(): Promise<void> {
    this.status = "disconnected";
    this.localParticipant = null;
    this.publishedTracks.clear();
    this.emit("connectionStatusChanged", "disconnected");
    this.telemetry?.log("info", "ortc_disconnected");
  }

  public getConnectionStatus(): ConnectionStatus {
    return this.status;
  }

  public getRoomId(): string {
    return this.roomId;
  }

  public getLocalParticipant(): ParticipantModel | null {
    return this.localParticipant;
  }

  public async publishTrack(
    track: MediaStreamTrack,
    options: PublishTrackOptions
  ): Promise<TrackDescriptor> {
    const trackId = `ortc-track-${options.source}-${Math.random().toString(36).substring(2, 7)}`;
    const descriptor: TrackDescriptor = {
      id: trackId,
      kind: track.kind as TrackKind,
      source: options.source,
      participantId: this.localParticipant?.id || "ortc-local-peer",
      isMuted: false,
      isEnabled: true,
      mediaStreamTrack: track,
    };
    this.publishedTracks.set(trackId, descriptor);
    return descriptor;
  }

  public async unpublishTrack(trackId: string): Promise<void> {
    this.publishedTracks.delete(trackId);
  }

  public async setTrackMuted(source: TrackSource, muted: boolean): Promise<void> {
    this.mutedSources.set(source, muted);
    for (const track of this.publishedTracks.values()) {
      if (track.source === source) {
        track.isMuted = muted;
        this.emit("trackMutedChanged", track.id, track.participantId, muted);
      }
    }
  }

  public isTrackMuted(source: TrackSource): boolean {
    return this.mutedSources.get(source) ?? false;
  }

  public async setTrackEnabled(trackId: string, enabled: boolean): Promise<void> {
    this.enabledTracks.set(trackId, enabled);
    // In ORTC protocol, this sends a signaling pause/resume message to SFU
    this.telemetry?.log("debug", "ortc_track_enabled_changed", {
      trackId,
      enabled,
    });
  }

  public async setVideoQuality(
    trackId: string,
    quality: VideoQualityLayer
  ): Promise<void> {
    this.qualityLayers.set(trackId, quality);
    // In ORTC protocol, this sends setPreferredLayers command to SFU
    this.telemetry?.log("debug", "ortc_quality_layer_changed", {
      trackId,
      quality,
    });
  }
}
