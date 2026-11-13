import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { BreakoutRoomsManager } from "../BreakoutRoomsManager";

describe("BreakoutRoomsManager Component", () => {
  it("allows host to add rooms, auto distribute, and start breakouts", async () => {
    const onSendData = vi.fn().mockResolvedValue(undefined);
    const allParticipants = [
      { id: "u-1", name: "User 1" },
      { id: "u-2", name: "User 2" },
      { id: "u-3", name: "User 3" },
      { id: "u-4", name: "User 4" },
    ];

    render(
      <BreakoutRoomsManager
        isHost={true}
        allParticipants={allParticipants}
        currentParticipantId="u-1"
        onSendData={onSendData}
      />
    );

    expect(screen.getByTestId("breakout-rooms-manager")).toBeInTheDocument();
    expect(screen.getByText("Breakout Room 1")).toBeInTheDocument();
    expect(screen.getByText("Breakout Room 2")).toBeInTheDocument();

    // Auto distribute
    fireEvent.click(screen.getByRole("button", { name: /auto distribute/i }));
    expect(onSendData).toHaveBeenCalled();

    // Start breakouts
    fireEvent.click(screen.getByRole("button", { name: /start breakouts/i }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /close all breakout rooms/i })).toBeInTheDocument();
    });
  });

  it("allows participant to join assigned breakout room and return", () => {
    const onJoinSubRoom = vi.fn();
    const onReturnToMainRoom = vi.fn();

    render(
      <BreakoutRoomsManager
        isHost={true}
        allParticipants={[{ id: "u-1", name: "User 1" }]}
        currentParticipantId="u-1"
        onJoinSubRoom={onJoinSubRoom}
        onReturnToMainRoom={onReturnToMainRoom}
      />
    );

    const joinBtns = screen.getAllByRole("button", { name: /join room/i });
    fireEvent.click(joinBtns[0]);
    expect(onJoinSubRoom).toHaveBeenCalledWith("room-1");

    // Return to main
    const returnBtn = screen.getByRole("button", { name: /return to main/i });
    fireEvent.click(returnBtn);
    expect(onReturnToMainRoom).toHaveBeenCalled();
  });
});
