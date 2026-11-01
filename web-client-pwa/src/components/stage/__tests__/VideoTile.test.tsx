import React from "react";
import { render, screen } from "@testing-library/react";
import { describe, it, expect, beforeEach } from "vitest";
import { VideoTile } from "../VideoTile";
import { useParticipantsStore } from "../../../store/use_participants_store";
import { useTelemetryStore } from "../../../store/use_telemetry_store";

describe("VideoTile Component", () => {
  beforeEach(() => {
    useParticipantsStore.getState().clearParticipants();
    useTelemetryStore.getState().clearTelemetry();

    useParticipantsStore.getState().upsertParticipant({
      id: "p-test-1",
      identity: "participant1",
      name: "Dave",
      isLocal: false,
      role: "speaker",
      isSpeaking: false,
      audioLevel: 0,
      connectionQuality: "good",
      isMicMuted: true,
      isCameraMuted: true,
      isScreenSharing: false,
      joinedAt: Date.now(),
    });
  });

  it("renders participant name and mic muted icon", () => {
    render(<VideoTile participantId="p-test-1" />);

    expect(screen.getAllByText("Dave").length).toBeGreaterThan(0);
    expect(screen.getByLabelText("Microphone muted")).toBeInTheDocument();
  });

  it("renders active speaker ring when participant is speaking", () => {
    useTelemetryStore.getState().setActiveSpeakers(["p-test-1"]);

    render(<VideoTile participantId="p-test-1" />);

    const tile = screen.getByTestId("video-tile-p-test-1");
    expect(tile.className).toContain("border-emerald-500");
  });
});
