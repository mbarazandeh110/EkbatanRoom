import { create } from "zustand";
import { ConnectionStatus, LayoutMode } from "../domain/models/room";

export interface RoomStoreState {
  roomId: string | null;
  roomName: string | null;
  status: ConnectionStatus;
  layoutMode: LayoutMode;
  localParticipantId: string | null;
  remoteParticipantIds: string[];
  error: string | null;

  // Actions
  setRoomInfo: (roomId: string, roomName?: string) => void;
  setConnectionStatus: (status: ConnectionStatus, error?: string | null) => void;
  setLayoutMode: (mode: LayoutMode) => void;
  setLocalParticipantId: (id: string | null) => void;
  addRemoteParticipantId: (id: string) => void;
  removeRemoteParticipantId: (id: string) => void;
  setError: (error: string | null) => void;
  resetRoom: () => void;
}

const initialState = {
  roomId: null,
  roomName: null,
  status: "disconnected" as ConnectionStatus,
  layoutMode: "grid" as LayoutMode,
  localParticipantId: null,
  remoteParticipantIds: [],
  error: null,
};

export const useRoomStore = create<RoomStoreState>((set) => ({
  ...initialState,

  setRoomInfo: (roomId, roomName) =>
    set({ roomId, roomName: roomName || roomId }),

  setConnectionStatus: (status, error = null) =>
    set((state) => ({
      status,
      error: error !== undefined ? error : state.error,
    })),

  setLayoutMode: (layoutMode) => set({ layoutMode }),

  setLocalParticipantId: (localParticipantId) => set({ localParticipantId }),

  addRemoteParticipantId: (id) =>
    set((state) => {
      if (state.remoteParticipantIds.includes(id)) return state;
      return { remoteParticipantIds: [...state.remoteParticipantIds, id] };
    }),

  removeRemoteParticipantId: (id) =>
    set((state) => ({
      remoteParticipantIds: state.remoteParticipantIds.filter((pId) => pId !== id),
    })),

  setError: (error) => set({ error }),

  resetRoom: () => set(initialState),
}));
