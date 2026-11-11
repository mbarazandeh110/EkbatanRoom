import type { RtpMetricsSnapshot } from '../domain/qoe_metrics.js';
import type { ReceivedDataMessage, MetadataChangeEvent, MutedTrackEvent, PermissionChangeEvent } from './window.js';

export interface TokenOptions {
  name?: string;
  metadata?: string;
  ttlSeconds?: number;
}

export interface ITokenFactory {
  createToken(roomId: string, identity: string, options?: TokenOptions): Promise<string>;
}

export interface JoinOptions {
  autoSubscribe?: boolean;
  publishAudio?: boolean;
  publishVideo?: boolean;
}

export interface IParticipantController {
  join(url: string, token: string, options?: JoinOptions): Promise<void>;
  publishTracks(options: { audio?: boolean; video?: boolean; simulcast?: boolean }): Promise<void>;
  setMicrophoneEnabled(enabled: boolean): Promise<void>;
  setCameraEnabled(enabled: boolean): Promise<void>;
  setVideoQuality(quality: 'low' | 'medium' | 'high'): Promise<void>;
  restartIce(): Promise<void>;
  waitForRemoteParticipant(participantIdentity?: string, timeoutMs?: number): Promise<void>;
  waitForRemoteTrack(kind: 'audio' | 'video', timeoutMs?: number): Promise<void>;
  publishData(
    data: string | number[] | Uint8Array,
    options?: { reliable?: boolean; topic?: string; destinationIdentities?: string[] }
  ): Promise<void>;
  getReceivedDataMessages(): Promise<ReceivedDataMessage[]>;
  clearDataMessages(): Promise<void>;
  getMetadataChanges(): Promise<MetadataChangeEvent[]>;
  getMutedTrackEvents(): Promise<MutedTrackEvent[]>;
  getPermissionChanges(): Promise<PermissionChangeEvent[]>;
  getStats(): Promise<RtpMetricsSnapshot>;
  leave(): Promise<void>;
}
