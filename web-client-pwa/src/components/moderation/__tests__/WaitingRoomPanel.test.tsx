import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { WaitingRoomPanel } from "../WaitingRoomPanel";

describe("WaitingRoomPanel Component", () => {
  it("renders pending guests and handles individual and all admissions", () => {
    const onAdmit = vi.fn();
    const onAdmitAll = vi.fn();
    const onReject = vi.fn();
    const onToggleWaitingRoom = vi.fn();

    const pendingGuests = [
      { id: "g-1", name: "Guest One", identity: "g1", requestedAt: Date.now() - 5000 },
      { id: "g-2", name: "Guest Two", identity: "g2", requestedAt: Date.now() - 3000 },
    ];

    render(
      <WaitingRoomPanel
        isWaitingRoomEnabled={true}
        onToggleWaitingRoom={onToggleWaitingRoom}
        pendingGuests={pendingGuests}
        onAdmit={onAdmit}
        onAdmitAll={onAdmitAll}
        onReject={onReject}
      />
    );

    expect(screen.getByTestId("waiting-room-panel")).toBeInTheDocument();
    expect(screen.getByText("Guest One")).toBeInTheDocument();
    expect(screen.getByText("Guest Two")).toBeInTheDocument();

    // Admit single
    const admitBtn = screen.getByRole("button", { name: /admit guest one/i });
    fireEvent.click(admitBtn);
    expect(onAdmit).toHaveBeenCalledWith("g-1");

    // Admit all
    const admitAllBtn = screen.getByRole("button", { name: /admit all/i });
    fireEvent.click(admitAllBtn);
    expect(onAdmitAll).toHaveBeenCalled();

    // Reject
    const rejectBtn = screen.getByRole("button", { name: /reject guest two/i });
    fireEvent.click(rejectBtn);
    expect(onReject).toHaveBeenCalledWith("g-2");
  });
});
