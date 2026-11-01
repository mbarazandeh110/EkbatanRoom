import { create } from "zustand";
import {
  ParticipantModel,
  ConnectionQualityLevel,
} from "../domain/models/participant";
import { TrackDescriptor, TrackSource } from "../domain/models/media";

export interface ParticipantsStoreState {
  participants: Record<string, ParticipantModel>;
  tracks: Record<string, TrackDescriptor>;
  participantTracks: Record<string, string[]>; // participantId -> trackId[]

  // Actions
  upsertParticipant: (participant: ParticipantModel) => void;
  removeParticipant: (participantId: string) => void;
  upsertTrack: (track: TrackDescriptor) => void;
  removeTrack: (trackId: string, participantId?: string) => void;
  setTrackMuted: (trackId: string, isMuted: boolean) => void;
  setParticipantQuality: (
    participantId: string,
    quality: ConnectionQualityLevel
  ) => void;
  setParticipantMuteState: (
    participantId: string,
    source: TrackSource,
    isMuted: boolean
  ) => void;
  clearParticipants: () => void;
}

export const useParticipantsStore = create<ParticipantsStoreState>((set) => ({
  participants: {},
  tracks: {},
  participantTracks: {},

  upsertParticipant: (participant) =>
    set((state) => ({
      participants: {
        ...state.participants,
        [participant.id]: {
          ...(state.participants[participant.id] || {}),
          ...participant,
        },
      },
    })),

  removeParticipant: (participantId) =>
    set((state) => {
      const nextParticipants = { ...state.participants };
      delete nextParticipants[participantId];

      const trackIdsToRemove = state.participantTracks[participantId] || [];
      const nextTracks = { ...state.tracks };
      trackIdsToRemove.forEach((id) => delete nextTracks[id]);

      const nextParticipantTracks = { ...state.participantTracks };
      delete nextParticipantTracks[participantId];

      return {
        participants: nextParticipants,
        tracks: nextTracks,
        participantTracks: nextParticipantTracks,
      };
    }),

  upsertTrack: (track) =>
    set((state) => {
      const existingTrackIds = state.participantTracks[track.participantId] || [];
      const updatedTrackIds = existingTrackIds.includes(track.id)
        ? existingTrackIds
        : [...existingTrackIds, track.id];

      // Also update participant's mute flags if it is camera or mic
      const participant = state.participants[track.participantId];
      let updatedParticipant = participant;
      if (participant) {
        if (track.source === "microphone") {
          updatedParticipant = { ...participant, isMicMuted: track.isMuted };
        } else if (track.source === "camera") {
          updatedParticipant = { ...participant, isCameraMuted: track.isMuted };
        } else if (track.source === "screen") {
          updatedParticipant = { ...participant, isScreenSharing: !track.isMuted };
        }
      }

      return {
        tracks: {
          ...state.tracks,
          [track.id]: track,
        },
        participantTracks: {
          ...state.participantTracks,
          [track.participantId]: updatedTrackIds,
        },
        participants: updatedParticipant
          ? {
              ...state.participants,
              [track.participantId]: updatedParticipant,
            }
          : state.participants,
      };
    }),

  removeTrack: (trackId, participantId) =>
    set((state) => {
      const targetTrack = state.tracks[trackId];
      const pId = participantId || targetTrack?.participantId;

      const nextTracks = { ...state.tracks };
      delete nextTracks[trackId];

      const nextParticipantTracks = { ...state.participantTracks };
      if (pId && nextParticipantTracks[pId]) {
        nextParticipantTracks[pId] = nextParticipantTracks[pId].filter(
          (id) => id !== trackId
        );
      }

      return {
        tracks: nextTracks,
        participantTracks: nextParticipantTracks,
      };
    }),

  setTrackMuted: (trackId, isMuted) =>
    set((state) => {
      const track = state.tracks[trackId];
      if (!track) return state;

      const updatedTrack = { ...track, isMuted };
      const participant = state.participants[track.participantId];
      let updatedParticipant = participant;

      if (participant) {
        if (track.source === "microphone") {
          updatedParticipant = { ...participant, isMicMuted: isMuted };
        } else if (track.source === "camera") {
          updatedParticipant = { ...participant, isCameraMuted: isMuted };
        } else if (track.source === "screen") {
          updatedParticipant = { ...participant, isScreenSharing: !isMuted };
        }
      }

      return {
        tracks: {
          ...state.tracks,
          [trackId]: updatedTrack,
        },
        participants: updatedParticipant
          ? {
              ...state.participants,
              [track.participantId]: updatedParticipant,
            }
          : state.participants,
      };
    }),

  setParticipantQuality: (participantId, connectionQuality) =>
    set((state) => {
      const participant = state.participants[participantId];
      if (!participant) return state;

      return {
        participants: {
          ...state.participants,
          [participantId]: {
            ...participant,
            connectionQuality,
          },
        },
      };
    }),

  setParticipantMuteState: (participantId, source, isMuted) =>
    set((state) => {
      const participant = state.participants[participantId];
      if (!participant) return state;

      const update: Partial<ParticipantModel> = {};
      if (source === "microphone") update.isMicMuted = isMuted;
      if (source === "camera") update.isCameraMuted = isMuted;
      if (source === "screen") update.isScreenSharing = !isMuted;

      return {
        participants: {
          ...state.participants,
          [participantId]: {
            ...participant,
            ...update,
          },
        },
      };
    }),

  clearParticipants: () =>
    set({
      participants: {},
      tracks: {},
      participantTracks: {},
    }),
}));

// Atomic Selectors for fine-grained subscriptions
export const useParticipant = (id: string | null): ParticipantModel | undefined =>
  useParticipantsStore((state) => (id ? state.participants[id] : undefined));

export const useParticipantTrack = (
  participantId: string | null,
  source: TrackSource
): TrackDescriptor | undefined =>
  useParticipantsStore((state) => {
    if (!participantId) return undefined;
    const trackIds = state.participantTracks[participantId] || [];
    for (const id of trackIds) {
      const t = state.tracks[id];
      if (t && t.source === source) {
        return t;
      }
    }
    return undefined;
  });

export const useParticipantQuality = (
  participantId: string | null
): ConnectionQualityLevel =>
  useParticipantsStore(
    (state) =>
      (participantId && state.participants[participantId]?.connectionQuality) ||
      "unknown"
  );
