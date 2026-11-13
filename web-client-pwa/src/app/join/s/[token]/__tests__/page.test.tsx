import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import GuestJoinPage from "../page";

// Mock next/navigation
const mockPush = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({
    push: mockPush,
  }),
}));

describe("GuestJoinPage", () => {
  it("decodes token and renders PreJoinLobby with room info", async () => {
    // Generate valid base64 payload
    const payload = JSON.stringify({
      roomId: "team-weekly",
      roomName: "Weekly Team Standup",
      name: "Guest Alice",
    });
    const fakeToken = `header.${btoa(payload)}.signature`;
    const tokenPromise = Promise.resolve({ token: fakeToken });

    render(<GuestJoinPage params={tokenPromise} />);

    await waitFor(() => {
      expect(screen.getByText("Weekly Team Standup")).toBeInTheDocument();
    });

    const nameInput = screen.getByLabelText("Your Name") as HTMLInputElement;
    expect(nameInput.value).toBe("Guest Alice");
  });
});
