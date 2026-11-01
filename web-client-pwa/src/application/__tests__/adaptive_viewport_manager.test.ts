import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { AdaptiveViewportManager } from "../adaptive_viewport_manager";
import { RemoteSubscriptionPort } from "../../ports/rtc_engine_port";

describe("AdaptiveViewportManager", () => {
  let mockPort: RemoteSubscriptionPort;
  let manager: AdaptiveViewportManager;

  beforeEach(() => {
    vi.useFakeTimers();
    mockPort = {
      setTrackEnabled: vi.fn().mockResolvedValue(undefined),
      setVideoQuality: vi.fn().mockResolvedValue(undefined),
    };
    manager = new AdaptiveViewportManager(mockPort, undefined, 200);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("calculates quality layer based on rendered width thresholds", () => {
    expect(AdaptiveViewportManager.calculateQualityLayer(200)).toBe("low");
    expect(AdaptiveViewportManager.calculateQualityLayer(279)).toBe("low");
    expect(AdaptiveViewportManager.calculateQualityLayer(280)).toBe("medium");
    expect(AdaptiveViewportManager.calculateQualityLayer(500)).toBe("medium");
    expect(AdaptiveViewportManager.calculateQualityLayer(640)).toBe("medium");
    expect(AdaptiveViewportManager.calculateQualityLayer(641)).toBe("high");
    expect(AdaptiveViewportManager.calculateQualityLayer(1920)).toBe("high");
  });

  it("applies debounced quality changes when intersecting", async () => {
    manager.onViewportChange("track-1", true, { width: 320, height: 240 });

    // Should not call immediately before debounce
    expect(mockPort.setVideoQuality).not.toHaveBeenCalled();

    // Fast-forward past debounce
    await vi.advanceTimersByTimeAsync(200);

    expect(mockPort.setVideoQuality).toHaveBeenCalledWith("track-1", "medium");
  });

  it("disables track when viewport goes off-screen", async () => {
    // Initial visible state
    manager.onViewportChange("track-2", true, { width: 800, height: 600 });
    await vi.advanceTimersByTimeAsync(200);
    expect(mockPort.setVideoQuality).toHaveBeenCalledWith("track-2", "high");

    // Goes off screen
    manager.onViewportChange("track-2", false, { width: 800, height: 600 });
    await vi.advanceTimersByTimeAsync(200);

    expect(mockPort.setTrackEnabled).toHaveBeenCalledWith("track-2", false);
  });

  it("re-enables track and applies quality when scrolling back on-screen", async () => {
    // Off screen
    manager.onViewportChange("track-3", false, { width: 800, height: 600 });
    await vi.advanceTimersByTimeAsync(200);
    expect(mockPort.setTrackEnabled).toHaveBeenCalledWith("track-3", false);

    // Scrolled back into view with smaller width
    manager.onViewportChange("track-3", true, { width: 250, height: 180 });
    await vi.advanceTimersByTimeAsync(200);

    expect(mockPort.setTrackEnabled).toHaveBeenCalledWith("track-3", true);
    expect(mockPort.setVideoQuality).toHaveBeenCalledWith("track-3", "low");
  });

  it("unregisters track and cancels pending timer", () => {
    manager.onViewportChange("track-4", true, { width: 500, height: 400 });
    manager.unregisterTrack("track-4");

    vi.advanceTimersByTime(200);
    expect(mockPort.setVideoQuality).not.toHaveBeenCalled();
    expect(manager.getTrackState("track-4")).toBeUndefined();
  });
});
