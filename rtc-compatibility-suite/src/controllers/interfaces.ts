import type { RtpMetricsSnapshot } from '../domain/qoe_metrics.js';

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
  publishTracks(options: { audio?: boolean; video?: boolean }): Promise<void>;
  setMicrophoneEnabled(enabled: boolean): Promise<void>;
  setCameraEnabled(enabled: boolean): Promise<void>;
  getStats(): Promise<RtpMetricsSnapshot>;
  leave(): Promise<void>;
}
