export type ParticipantRole = "host" | "speaker" | "listener";

export type ConnectionQualityLevel = "excellent" | "good" | "poor" | "lost" | "unknown";

export interface ParticipantModel {
  id: string;
  identity: string;
  name: string;
  isLocal: boolean;
  role: ParticipantRole;
  isSpeaking: boolean;
  audioLevel: number; // 0.0 to 1.0
  connectionQuality: ConnectionQualityLevel;
  isMicMuted: boolean;
  isCameraMuted: boolean;
  isScreenSharing: boolean;
  joinedAt: number;
}
