export type ConnectionStatus =
  | "disconnected"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "failed";

export type LayoutMode = "grid" | "speaker" | "presentation";

export interface RoomState {
  roomId: string;
  roomName?: string;
  status: ConnectionStatus;
  layoutMode: LayoutMode;
  localParticipantId: string | null;
  remoteParticipantIds: string[];
  activeSpeakerId: string | null;
  error: string | null;
}
