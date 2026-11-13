import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { RoomLockModal } from "../RoomLockModal";

describe("RoomLockModal Component", () => {
  it("renders when open and allows toggling restrictions", async () => {
    const onApplySettings = vi.fn();
    const onClose = vi.fn();
    const onSendData = vi.fn().mockResolvedValue(undefined);

    render(
      <RoomLockModal
        isOpen={true}
        onClose={onClose}
        onApplySettings={onApplySettings}
        onSendData={onSendData}
      />
    );

    expect(screen.getByTestId("room-lock-modal")).toBeInTheDocument();
    expect(screen.getByText("Room Lock & Permission Settings")).toBeInTheDocument();

    // Toggle mute all microphones
    const micLock = screen.getByText("Mute All Microphones");
    fireEvent.click(micLock);

    // Apply
    fireEvent.click(screen.getByRole("button", { name: /apply restrictions/i }));

    await waitFor(() => {
      expect(onApplySettings).toHaveBeenCalledWith(
        expect.objectContaining({ lockAllMics: true })
      );
      expect(onSendData).toHaveBeenCalledWith(
        expect.anything(),
        "room_lock_settings"
      );
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("does not render when isOpen is false", () => {
    const { container } = render(
      <RoomLockModal
        isOpen={false}
        onClose={vi.fn()}
        onApplySettings={vi.fn()}
      />
    );

    expect(container.firstChild).toBeNull();
  });
});
