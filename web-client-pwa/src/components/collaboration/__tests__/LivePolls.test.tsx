import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { LivePolls } from "../LivePolls";

describe("LivePolls Component", () => {
  it("allows host to create a new poll and broadcast it", async () => {
    const onSendData = vi.fn().mockResolvedValue(undefined);
    render(
      <LivePolls
        isHost={true}
        currentParticipantId="host-1"
        onSendData={onSendData}
      />
    );

    // Open creation form
    fireEvent.click(screen.getByRole("button", { name: /create new poll/i }));

    const questionInput = screen.getByPlaceholderText(/which webrtc codec/i);
    fireEvent.change(questionInput, { target: { value: "Favorite Codec?" } });

    fireEvent.click(screen.getByRole("button", { name: /launch poll/i }));

    await waitFor(() => {
      expect(screen.getByText("Favorite Codec?")).toBeInTheDocument();
      expect(onSendData).toHaveBeenCalledWith(
        expect.anything(),
        "room_polls"
      );
    });
  });

  it("allows user to cast a vote on an active poll", async () => {
    const onSendData = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <LivePolls
        isHost={false}
        currentParticipantId="user-2"
        onSendData={onSendData}
      />
    );

    const poll = {
      id: "poll-101",
      question: "VP9 or AV1?",
      options: [
        { id: "opt-1", text: "VP9", votes: 0 },
        { id: "opt-2", text: "AV1", votes: 0 },
      ],
      isClosed: false,
      isQuiz: false,
      createdBy: "host-1",
      createdAt: Date.now(),
      totalVotes: 0,
    };

    const payload = {
      type: "create",
      pollId: poll.id,
      poll,
    };
    const bytes = new TextEncoder().encode(JSON.stringify(payload));

    rerender(
      <LivePolls
        isHost={false}
        currentParticipantId="user-2"
        onSendData={onSendData}
        incomingData={{ data: bytes, topic: "room_polls" }}
      />
    );

    expect(screen.getByText("VP9 or AV1?")).toBeInTheDocument();

    const optButton = screen.getByRole("button", { name: /vp9/i });
    fireEvent.click(optButton);

    await waitFor(() => {
      expect(screen.getByText("Vote Recorded")).toBeInTheDocument();
      expect(onSendData).toHaveBeenCalledWith(
        expect.anything(),
        "room_polls"
      );
    });
  });
});
