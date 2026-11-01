import {
  RtcEnginePort,
  ConnectOptions,
  PublishTrackOptions,
  RtcEngineEvents,
  ActiveSpeakerInfo,
} from "../../ports/rtc_engine_port";
import {
  ConnectionStatus,
  ParticipantModel,
  TrackDescriptor,
  TrackSource,
  VideoQualityLayer,
  ConnectionQualityLevel,
} from "../../ports/rtc_engine_port";

export class MockRtcEngineAdapter implements RtcEnginePort {
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

    this.roomId = options.roomName || "mock-room-123";
    this.localParticipant = {
      id: "local-user-1",
      identity: options.participantName || "local-user",
      name: options.participantName || "Local User",
      isLocal: true,
      role: "host",
      isSpeaking: false,
      audioLevel: 0,
      connectionQuality: "excellent",
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
    const trackId = `track-${options.source}-${Math.random().toString(36).substring(2, 7)}`;
    const descriptor: TrackDescriptor = {
      id: trackId,
      kind: track.kind as "audio" | "video",
      source: options.source,
      participantId: this.localParticipant?.id || "local-user-1",
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
  }

  public isTrackEnabled(trackId: string): boolean {
    return this.enabledTracks.get(trackId) ?? true;
  }

  public async setVideoQuality(
    trackId: string,
    quality: VideoQualityLayer
  ): Promise<void> {
    this.qualityLayers.set(trackId, quality);
  }

  public getVideoQuality(trackId: string): VideoQualityLayer | undefined {
    return this.qualityLayers.get(trackId);
  }

  // Simulation helpers for tests
  public simulateRemoteParticipantJoin(participant: ParticipantModel): void {
    this.emit("participantJoined", participant);
  }

  public simulateRemoteParticipantLeave(participantId: string): void {
    this.emit("participantLeft", participantId);
  }

  public simulateTrackSubscribed(
    track: TrackDescriptor,
    participant: ParticipantModel
  ): void {
    this.emit("trackSubscribed", track, participant);
  }

  public simulateActiveSpeakers(speakers: ActiveSpeakerInfo[]): void {
    this.emit("activeSpeakersChanged", speakers);
  }

  public simulateAudioLevels(levels: Record<string, number>): void {
    this.emit("audioLevelsChanged", levels);
  }

  public simulateConnectionQuality(
    participantId: string,
    quality: ConnectionQualityLevel
  ): void {
    this.emit("connectionQualityChanged", participantId, quality);
  }
}
