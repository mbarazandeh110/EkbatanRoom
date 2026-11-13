import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { QnAPanel } from "../QnAPanel";

describe("QnAPanel Component", () => {
  it("allows user to ask a question and broadcasts it", async () => {
    const onSendData = vi.fn().mockResolvedValue(undefined);
    render(
      <QnAPanel
        isHost={false}
        currentParticipant={{ id: "user-1", name: "Alice" }}
        onSendData={onSendData}
      />
    );

    const input = screen.getByPlaceholderText(/ask a question for speaker/i);
    fireEvent.change(input, { target: { value: "How is simulcast configured?" } });

    fireEvent.click(screen.getByRole("button", { name: /submit question/i }));

    await waitFor(() => {
      expect(screen.getByText("How is simulcast configured?")).toBeInTheDocument();
      expect(onSendData).toHaveBeenCalledWith(
        expect.anything(),
        "room_qna"
      );
    });
  });

  it("handles question upvoting and host mark answered", async () => {
    const onSendData = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <QnAPanel
        isHost={true}
        currentParticipant={{ id: "host-1", name: "Teacher" }}
        onSendData={onSendData}
      />
    );

    const question = {
      id: "q-1",
      askerId: "user-2",
      askerName: "Bob",
      text: "Is Safari supported?",
      upvotes: 0,
      upvotedBy: [],
      isAnswered: false,
      createdAt: Date.now(),
    };

    const bytes = new TextEncoder().encode(
      JSON.stringify({ type: "ask", question })
    );

    rerender(
      <QnAPanel
        isHost={true}
        currentParticipant={{ id: "host-1", name: "Teacher" }}
        onSendData={onSendData}
        incomingData={{ data: bytes, topic: "room_qna" }}
      />
    );

    expect(screen.getByText("Is Safari supported?")).toBeInTheDocument();

    // Upvote
    const upvoteBtn = screen.getByRole("button", { name: /upvote question/i });
    fireEvent.click(upvoteBtn);

    await waitFor(() => {
      expect(onSendData).toHaveBeenCalledWith(
        expect.anything(),
        "room_qna"
      );
    });

    // Host marks answered
    const markAnsweredBtn = screen.getByRole("button", { name: /mark as answered/i });
    fireEvent.click(markAnsweredBtn);

    await waitFor(() => {
      expect(screen.getByText("Answered")).toBeInTheDocument();
    });
  });
});
