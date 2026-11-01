import { create } from "zustand";
import { WebRtcQoESnapshot } from "../ports/telemetry_port";

export interface TelemetryStoreState {
  audioLevels: Record<string, number>; // participantId -> 0.0 to 1.0
  activeSpeakerIds: string[];
  latestQoE: WebRtcQoESnapshot | null;

  // Actions
  setAudioLevels: (levels: Record<string, number>) => void;
  setActiveSpeakers: (speakerIds: string[]) => void;
  setQoESnapshot: (snapshot: WebRtcQoESnapshot) => void;
  clearTelemetry: () => void;
}

export const useTelemetryStore = create<TelemetryStoreState>((set) => ({
  audioLevels: {},
  activeSpeakerIds: [],
  latestQoE: null,

  setAudioLevels: (levels) =>
    set((state) => ({
      audioLevels: {
        ...state.audioLevels,
        ...levels,
      },
    })),

  setActiveSpeakers: (activeSpeakerIds) => set({ activeSpeakerIds }),

  setQoESnapshot: (latestQoE) => set({ latestQoE }),

  clearTelemetry: () =>
    set({
      audioLevels: {},
      activeSpeakerIds: [],
      latestQoE: null,
    }),
}));

// Atomic Selectors for high-frequency subscriptions
export const useParticipantAudioLevel = (participantId: string | null): number =>
  useTelemetryStore(
    (state) => (participantId ? state.audioLevels[participantId] ?? 0 : 0)
  );

export const useIsParticipantSpeaking = (participantId: string | null): boolean =>
  useTelemetryStore(
    (state) =>
      Boolean(participantId && state.activeSpeakerIds.includes(participantId))
  );

export const useLatestQoESnapshot = (): WebRtcQoESnapshot | null =>
  useTelemetryStore((state) => state.latestQoE);
