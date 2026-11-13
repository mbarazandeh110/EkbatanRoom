import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ModeratedChat } from "../ModeratedChat";

describe("ModeratedChat Component", () => {
  beforeEach(() => {
    window.HTMLElement.prototype.scrollIntoView = vi.fn();
  });

  it("sends public message and displays it in chat", async () => {
    const onSendData = vi.fn().mockResolvedValue(undefined);
    render(
      <ModeratedChat
        roomId="room-1"
        isHost={false}
        currentParticipant={{ id: "user-1", name: "Alice" }}
        onSendData={onSendData}
      />
    );

    const input = screen.getByPlaceholderText(/type a message/i);
    fireEvent.change(input, { target: { value: "Hello everyone!" } });

    fireEvent.click(screen.getByRole("button", { name: /send message/i }));

    await waitFor(() => {
      expect(screen.getByText("Hello everyone!")).toBeInTheDocument();
      expect(onSendData).toHaveBeenCalledWith(
        expect.anything(),
        "room_chat"
      );
    });
  });

  it("handles operator moderation queue and approvals for host", async () => {
    const onSendData = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <ModeratedChat
        roomId="room-1"
        isHost={true}
        currentParticipant={{ id: "host-1", name: "Teacher" }}
        onSendData={onSendData}
      />
    );

    // Toggle moderation ON
    const modBtn = screen.getByRole("button", { name: /toggle chat moderation/i });
    fireEvent.click(modBtn);

    expect(screen.getByText("Moderation: ON")).toBeInTheDocument();

    // Receive a pending message from student
    const pendingMsg = {
      id: "msg-p1",
      senderId: "student-1",
      senderName: "Bob",
      text: "Can you repeat slide 2?",
      timestamp: Date.now(),
      status: "pending",
    };
    const bytes = new TextEncoder().encode(
      JSON.stringify({ type: "new_message", message: pendingMsg })
    );

    rerender(
      <ModeratedChat
        roomId="room-1"
        isHost={true}
        currentParticipant={{ id: "host-1", name: "Teacher" }}
        onSendData={onSendData}
        incomingData={{ data: bytes, topic: "room_chat" }}
      />
    );

    // Switch to Queue tab
    fireEvent.click(screen.getByRole("button", { name: /queue/i }));
    expect(screen.getByText("Can you repeat slide 2?")).toBeInTheDocument();

    // Approve message
    fireEvent.click(screen.getByRole("button", { name: /approve message/i }));

    await waitFor(() => {
      expect(onSendData).toHaveBeenCalledWith(
        expect.anything(),
        "room_chat"
      );
    });
  });

  it("handles raised hands queue with stage promotion for host", () => {
    const onPromoteToStage = vi.fn();
    const onLowerHand = vi.fn();

    render(
      <ModeratedChat
        roomId="room-1"
        isHost={true}
        currentParticipant={{ id: "host-1", name: "Teacher" }}
        raisedHands={[{ id: "student-1", name: "Student 1" }]}
        onPromoteToStage={onPromoteToStage}
        onLowerHand={onLowerHand}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: /stage \(1\)/i }));

    expect(screen.getByText("Student 1")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /promote to stage/i }));
    expect(onPromoteToStage).toHaveBeenCalledWith("student-1");
  });
});
