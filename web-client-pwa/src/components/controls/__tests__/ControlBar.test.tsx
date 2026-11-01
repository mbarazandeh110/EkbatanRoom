import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ControlBar } from "../ControlBar";
import { useRoomStore } from "../../../store/use_room_store";
import { useParticipantsStore } from "../../../store/use_participants_store";
import { MockRtcEngineAdapter } from "../../../adapters/mock/mock_rtc_engine_adapter";
import { RoomSessionController } from "../../../application/room_session_controller";

describe("ControlBar Component", () => {
  let engine: MockRtcEngineAdapter;
  let controller: RoomSessionController;

  beforeEach(() => {
    useRoomStore.getState().resetRoom();
    useParticipantsStore.getState().clearParticipants();

    engine = new MockRtcEngineAdapter();
    controller = new RoomSessionController(engine);

    useRoomStore.getState().setLocalParticipantId("local-user-1");
    useParticipantsStore.getState().upsertParticipant({
      id: "local-user-1",
      identity: "me",
      name: "Me",
      isLocal: true,
      role: "speaker",
      isSpeaking: false,
      audioLevel: 0,
      connectionQuality: "excellent",
      isMicMuted: false,
      isCameraMuted: false,
      isScreenSharing: false,
      joinedAt: Date.now(),
    });
  });

  it("toggles microphone on button click and updates aria-label", async () => {
    render(<ControlBar controller={controller} />);

    const micBtn = screen.getByRole("button", { name: /mute microphone/i });
    expect(micBtn).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(micBtn);

    await waitFor(() => {
      expect(engine.isTrackMuted("microphone")).toBe(true);
      expect(screen.getByRole("button", { name: /unmute microphone/i })).toBeInTheDocument();
    });
  });

  it("toggles camera on button click", async () => {
    render(<ControlBar controller={controller} />);

    const camBtn = screen.getByRole("button", { name: /turn off camera/i });
    expect(camBtn).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(camBtn);

    await waitFor(() => {
      expect(engine.isTrackMuted("camera")).toBe(true);
    });
  });

  it("switches layout mode between grid and speaker", () => {
    render(<ControlBar controller={controller} />);

    expect(useRoomStore.getState().layoutMode).toBe("grid");

    const layoutBtn = screen.getByRole("button", { name: /switch to speaker layout/i });
    fireEvent.click(layoutBtn);

    expect(useRoomStore.getState().layoutMode).toBe("speaker");
  });

  it("triggers keyboard shortcut 'm' for microphone toggle", async () => {
    render(<ControlBar controller={controller} />);

    fireEvent.keyDown(window, { key: "m" });

    await waitFor(() => {
      expect(engine.isTrackMuted("microphone")).toBe(true);
    });
  });
});
