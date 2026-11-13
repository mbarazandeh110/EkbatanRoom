import React from "react";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { SynchronizedMediaPlayer } from "../SynchronizedMediaPlayer";

describe("SynchronizedMediaPlayer Component", () => {
  beforeEach(() => {
    if (typeof HTMLVideoElement !== "undefined") {
      HTMLVideoElement.prototype.play = vi.fn().mockResolvedValue(undefined);
      HTMLVideoElement.prototype.pause = vi.fn();
      HTMLVideoElement.prototype.load = vi.fn();
    }
  });

  it("renders host controls and URL loader for host", () => {
    render(<SynchronizedMediaPlayer isHost={true} />);

    expect(screen.getByTestId("sync-media-player")).toBeInTheDocument();
    expect(
      screen.getByPlaceholderText(/paste direct mp4\/webm video url/i)
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /load media/i })).toBeInTheDocument();
  });

  it("loads media URL and plays/pauses sharing via DataChannel", async () => {
    const onSendData = vi.fn().mockResolvedValue(undefined);
    render(
      <SynchronizedMediaPlayer
        isHost={true}
        initialMediaUrl="https://example.com/video.mp4"
        onSendData={onSendData}
      />
    );

    const playBtn = screen.getByRole("button", { name: /play shared media/i });
    fireEvent.click(playBtn);

    expect(onSendData).toHaveBeenCalledWith(
      expect.anything(),
      "media_sync"
    );
  });

  it("syncs viewer state upon receiving remote DataChannel sync packet", async () => {
    const { rerender } = render(
      <SynchronizedMediaPlayer
        isHost={false}
        initialMediaUrl="https://example.com/video.mp4"
      />
    );

    expect(screen.getByText(/paused \(host sync\)/i)).toBeInTheDocument();

    const payload = {
      action: "play",
      currentTime: 15.5,
      mediaUrl: "https://example.com/video.mp4",
      hostTimestamp: Date.now(),
    };
    const bytes = new TextEncoder().encode(JSON.stringify(payload));

    act(() => {
      rerender(
        <SynchronizedMediaPlayer
          isHost={false}
          initialMediaUrl="https://example.com/video.mp4"
          incomingData={{ data: bytes, topic: "media_sync" }}
        />
      );
    });

    expect(screen.getByText(/playing \(host sync\)/i)).toBeInTheDocument();
  });
});
