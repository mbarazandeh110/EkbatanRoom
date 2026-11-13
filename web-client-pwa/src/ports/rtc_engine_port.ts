import {
  ConnectionStatus,
  LayoutMode,
  RoomState,
} from "../domain/models/room";
import {
  ParticipantModel,
  ParticipantRole,
  ConnectionQualityLevel,
} from "../domain/models/participant";
import {
  TrackDescriptor,
  TrackSource,
  TrackKind,
  VideoQualityLayer,
} from "../domain/models/media";

export type { ConnectionStatus, LayoutMode, RoomState };
export type { ParticipantModel, ParticipantRole, ConnectionQualityLevel };
export type { TrackDescriptor, TrackSource, TrackKind, VideoQualityLayer };

export interface ConnectOptions {
  url: string;
  token: string;
  roomName?: string;
  participantName?: string;
  lanIp?: string;
}

export interface PublishTrackOptions {
  simulcast?: boolean;
  source: TrackSource;
}

export interface DataSendOptions {
  topic?: string;
  reliable?: boolean;
  destinationIdentities?: string[];
}

export interface RtcDataChannelPort {
  sendData(data: Uint8Array | string, options?: DataSendOptions): Promise<void>;
}

export interface RtcConnectionPort {
  connect(options: ConnectOptions): Promise<void>;
  disconnect(): Promise<void>;
  getConnectionStatus(): ConnectionStatus;
  getRoomId(): string;
  getLocalParticipant(): ParticipantModel | null;
  restartIce?(): Promise<void>;
}

export interface LocalMediaControlPort {
  publishTrack(track: MediaStreamTrack, options: PublishTrackOptions): Promise<TrackDescriptor>;
  unpublishTrack(trackId: string): Promise<void>;
  setTrackMuted(source: TrackSource, muted: boolean): Promise<void>;
  isTrackMuted(source: TrackSource): boolean;
}

export interface RemoteSubscriptionPort {
  setTrackEnabled(trackId: string, enabled: boolean): Promise<void>;
  setVideoQuality(trackId: string, quality: VideoQualityLayer): Promise<void>;
}

export interface ActiveSpeakerInfo {
  participantId: string;
  audioLevel: number; // 0.0 to 1.0
}

export interface RtcEngineEvents {
  connectionStatusChanged: (status: ConnectionStatus, error?: Error) => void;
  participantJoined: (participant: ParticipantModel) => void;
  participantLeft: (participantId: string) => void;
  trackSubscribed: (track: TrackDescriptor, participant: ParticipantModel) => void;
  trackUnsubscribed: (trackId: string, participantId: string) => void;
  trackMutedChanged: (trackId: string, participantId: string, isMuted: boolean) => void;
  activeSpeakersChanged: (speakers: ActiveSpeakerInfo[]) => void;
  connectionQualityChanged: (participantId: string, quality: ConnectionQualityLevel) => void;
  audioLevelsChanged: (levels: Record<string, number>) => void;
  dataReceived: (data: Uint8Array | string, participant?: ParticipantModel, topic?: string) => void;
  reconnecting: () => void;
  reconnected: () => void;
}

export interface RtcEventsPort {
  on<E extends keyof RtcEngineEvents>(event: E, handler: RtcEngineEvents[E]): void;
  off<E extends keyof RtcEngineEvents>(event: E, handler: RtcEngineEvents[E]): void;
  emit<E extends keyof RtcEngineEvents>(event: E, ...args: Parameters<RtcEngineEvents[E]>): void;
}

/**
 * Composite Port combining segregated ports (DIP and ISP compliant)
 */
export interface RtcEnginePort
  extends RtcConnectionPort,
    LocalMediaControlPort,
    RemoteSubscriptionPort,
    RtcDataChannelPort,
    RtcEventsPort {}
