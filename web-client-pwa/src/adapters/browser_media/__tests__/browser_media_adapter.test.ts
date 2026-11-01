import { describe, it, expect, vi, beforeEach } from "vitest";
import { BrowserMediaAdapter } from "../browser_media_adapter";

describe("BrowserMediaAdapter", () => {
  let adapter: BrowserMediaAdapter;

  beforeEach(() => {
    adapter = new BrowserMediaAdapter();
  });

  it("handles AudioContext unlocking", async () => {
    const mockResume = vi.fn().mockResolvedValue(undefined);
    (window as any).AudioContext = class {
      public state = "suspended";
      public async resume() {
        this.state = "running";
        return mockResume();
      }
    };

    const unlocked = await adapter.unlockAudioContext();
    expect(unlocked).toBe(true);
    expect(mockResume).toHaveBeenCalled();
  });

  it("handles WakeLock request and release", async () => {
    const releaseMock = vi.fn().mockResolvedValue(undefined);
    (navigator as any).wakeLock = {
      request: vi.fn().mockResolvedValue({
        release: releaseMock,
        addEventListener: vi.fn(),
      }),
    };

    const requested = await adapter.requestWakeLock();
    expect(requested).toBe(true);
    expect((navigator as any).wakeLock.request).toHaveBeenCalledWith("screen");

    await adapter.releaseWakeLock();
    expect(releaseMock).toHaveBeenCalled();
  });

  it("enumerates devices from navigator.mediaDevices", async () => {
    (navigator as any).mediaDevices = {
      enumerateDevices: vi.fn().mockResolvedValue([
        {
          deviceId: "mic-1",
          kind: "audioinput",
          label: "Built-in Microphone",
          groupId: "group-1",
        },
        {
          deviceId: "cam-1",
          kind: "videoinput",
          label: "FaceTime HD Camera",
          groupId: "group-2",
        },
      ]),
    };

    const devices = await adapter.enumerateDevices();
    expect(devices).toHaveLength(2);
    expect(devices[0].deviceId).toBe("mic-1");
    expect(devices[1].label).toBe("FaceTime HD Camera");
  });

  it("stops all tracks in MediaStream", () => {
    const track1 = { stop: vi.fn() };
    const track2 = { stop: vi.fn() };
    const mockStream = {
      getTracks: vi.fn().mockReturnValue([track1, track2]),
    } as unknown as MediaStream;

    adapter.stopStream(mockStream);
    expect(track1.stop).toHaveBeenCalled();
    expect(track2.stop).toHaveBeenCalled();
  });
});
