import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { PreJoinLobby } from "../PreJoinLobby";
import { MediaDevicesPort } from "../../../ports/media_devices_port";

describe("PreJoinLobby Component", () => {
  let mockMediaDevices: MediaDevicesPort;
  let mockOnJoin: any;

  beforeEach(() => {
    mockMediaDevices = {
      enumerateDevices: vi.fn().mockResolvedValue([
        {
          deviceId: "mic-1",
          label: "Studio Mic",
          kind: "audioinput",
        },
        {
          deviceId: "cam-1",
          label: "HD Webcam",
          kind: "videoinput",
        },
      ]),
      getUserMedia: vi.fn().mockResolvedValue({
        getVideoTracks: () => [{ stop: vi.fn() }],
        getAudioTracks: () => [{ stop: vi.fn() }],
        getTracks: () => [{ stop: vi.fn() }],
      } as unknown as MediaStream),
      getDisplayMedia: vi.fn(),
      unlockAudioContext: vi.fn().mockResolvedValue(true),
      requestWakeLock: vi.fn().mockResolvedValue(true),
      releaseWakeLock: vi.fn().mockResolvedValue(undefined),
      stopStream: vi.fn(),
    };

    mockOnJoin = vi.fn().mockResolvedValue(undefined);
  });

  it("renders lobby with room name and device options", async () => {
    render(
      <PreJoinLobby
        roomName="Engineering All-Hands"
        mediaDevices={mockMediaDevices}
        onJoin={mockOnJoin}
      />
    );

    expect(screen.getByText("Engineering All-Hands")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("e.g. Mahdi")).toBeInTheDocument();

    await waitFor(() => {
      expect(mockMediaDevices.enumerateDevices).toHaveBeenCalled();
    });
  });

  it("unlocks AudioContext and requests WakeLock upon explicit Join click", async () => {
    render(
      <PreJoinLobby
        roomName="Planning Room"
        mediaDevices={mockMediaDevices}
        onJoin={mockOnJoin}
      />
    );

    const nameInput = screen.getByPlaceholderText("e.g. Mahdi");
    fireEvent.change(nameInput, { target: { value: "Sara" } });

    const joinButton = screen.getByRole("button", { name: "Join Room" });
    fireEvent.click(joinButton);

    await waitFor(() => {
      // Must unlock iOS Safari AudioContext
      expect(mockMediaDevices.unlockAudioContext).toHaveBeenCalled();
      // Must request mobile WakeLock
      expect(mockMediaDevices.requestWakeLock).toHaveBeenCalled();
      // Must invoke onJoin callback
      expect(mockOnJoin).toHaveBeenCalledWith(
        expect.objectContaining({
          participantName: "Sara",
        })
      );
    });
  });
});
