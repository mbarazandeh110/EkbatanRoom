import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { FileSharingBox } from "../FileSharingBox";

describe("FileSharingBox Component", () => {
  beforeEach(() => {
    URL.createObjectURL = vi.fn().mockReturnValue("blob:mock-file-url");
  });

  it("renders empty state initially", () => {
    render(
      <FileSharingBox
        roomId="room-1"
        currentParticipant={{ id: "user-1", name: "Mahdi" }}
      />
    );

    expect(screen.getByTestId("file-sharing-box")).toBeInTheDocument();
    expect(screen.getByText("Room Files & Handouts")).toBeInTheDocument();
    expect(screen.getByText("No handouts shared yet in this room.")).toBeInTheDocument();
  });

  it("handles file upload and broadcasts via DataChannel", async () => {
    const onSendData = vi.fn().mockResolvedValue(undefined);
    render(
      <FileSharingBox
        roomId="room-1"
        currentParticipant={{ id: "user-1", name: "Mahdi" }}
        onSendData={onSendData}
      />
    );

    const input = document.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(["dummy content"], "slides.pdf", { type: "application/pdf" });

    fireEvent.change(input, { target: { files: [file] } });

    await waitFor(() => {
      expect(screen.getByText("slides.pdf")).toBeInTheDocument();
      expect(onSendData).toHaveBeenCalledWith(
        expect.anything(),
        "file_shared"
      );
    });
  });
});
