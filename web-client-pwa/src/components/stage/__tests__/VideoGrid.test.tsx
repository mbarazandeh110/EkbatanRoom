import React from "react";
import { render, screen } from "@testing-library/react";
import { describe, it, expect, beforeEach } from "vitest";
import { VideoGrid } from "../VideoGrid";
import { useRoomStore } from "../../../store/use_room_store";
import { useParticipantsStore } from "../../../store/use_participants_store";

describe("VideoGrid Component", () => {
  beforeEach(() => {
    useRoomStore.getState().resetRoom();
    useParticipantsStore.getState().clearParticipants();
  });

  it("renders empty state message when no participants joined", () => {
    render(<VideoGrid />);
    expect(
      screen.getByText("Waiting for participants to join...")
    ).toBeInTheDocument();
  });

  it("renders tiles for local and remote participants in grid mode", () => {
    useRoomStore.getState().setLocalParticipantId("local-1");
    useRoomStore.getState().addRemoteParticipantId("remote-1");
    useRoomStore.getState().addRemoteParticipantId("remote-2");

    useParticipantsStore.getState().upsertParticipant({
      id: "local-1",
      identity: "me",
      name: "Me",
      isLocal: true,
      role: "host",
      isSpeaking: false,
      audioLevel: 0,
      connectionQuality: "excellent",
      isMicMuted: false,
      isCameraMuted: true,
      isScreenSharing: false,
      joinedAt: 1,
    });

    render(<VideoGrid />);

    expect(screen.getByTestId("video-tile-local-1")).toBeInTheDocument();
    expect(screen.getByTestId("video-tile-remote-1")).toBeInTheDocument();
    expect(screen.getByTestId("video-tile-remote-2")).toBeInTheDocument();
  });

  it("switches to speaker mode layout", () => {
    useRoomStore.getState().setLocalParticipantId("local-1");
    useRoomStore.getState().addRemoteParticipantId("remote-1");
    useRoomStore.getState().setLayoutMode("speaker");

    render(<VideoGrid />);

    expect(screen.getByTestId("video-tile-local-1")).toBeInTheDocument();
    expect(screen.getByTestId("video-tile-remote-1")).toBeInTheDocument();
  });

  it("renders presentation mode with host lock indicator and watermark", () => {
    useRoomStore.getState().setLocalParticipantId("local-1");
    useRoomStore.getState().addRemoteParticipantId("remote-1");
    useParticipantsStore.getState().upsertParticipant({
      id: "local-1",
      identity: "prof-dr",
      name: "Professor",
      isLocal: true,
      role: "host",
      isSpeaking: false,
      audioLevel: 0,
      connectionQuality: "excellent",
      isMicMuted: false,
      isCameraMuted: true,
      isScreenSharing: false,
      joinedAt: 1,
    });

    render(
      <VideoGrid
        overrideLayoutMode="presentation"
        isHostSyncLocked={true}
        watermarkEnabled={true}
        watermarkUserPhone="+989120000000"
      />
    );

    expect(screen.getByText("Host Locked Presentation")).toBeInTheDocument();
    expect(screen.getByTestId("forensic-watermark")).toBeInTheDocument();
    expect(screen.getByText(/\+989120000000/)).toBeInTheDocument();
  });
});
