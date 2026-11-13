import { describe, it, expect, vi, beforeEach } from "vitest";
import { VirtualBackgroundProcessor } from "../virtual_background_processor";

describe("VirtualBackgroundProcessor", () => {
  let mockTrack: MediaStreamTrack;

  beforeEach(() => {
    mockTrack = {
      kind: "video",
      id: "video-track-1",
      stop: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      getSettings: vi.fn().mockReturnValue({ width: 640, height: 480 }),
    } as unknown as MediaStreamTrack;

    // Polyfill canvas 2d context and captureStream for jsdom
    if (typeof HTMLCanvasElement !== "undefined") {
      HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue({
        filter: "none",
        drawImage: vi.fn(),
        fillRect: vi.fn(),
        clearRect: vi.fn(),
      });

      HTMLCanvasElement.prototype.captureStream = vi.fn().mockReturnValue({
        getVideoTracks: () => [
          {
            kind: "video",
            id: "processed-video-track",
            stop: vi.fn(),
          },
        ],
      });
    }

    // Polyfill HTMLVideoElement play and readyState
    if (typeof HTMLVideoElement !== "undefined") {
      HTMLVideoElement.prototype.play = vi.fn().mockResolvedValue(undefined);
      HTMLVideoElement.prototype.pause = vi.fn();
      Object.defineProperty(HTMLVideoElement.prototype, "readyState", {
        value: 4,
        configurable: true,
      });
    }
  });

  it("passes through non-video tracks unchanged", async () => {
    const processor = new VirtualBackgroundProcessor();
    const audioTrack = { kind: "audio", id: "audio-1" } as unknown as MediaStreamTrack;
    const result = await processor.process(audioTrack);
    expect(result).toBe(audioTrack);
  });

  it("initializes video processor and captures stream from canvas", async () => {
    const processor = new VirtualBackgroundProcessor({ mode: "blur", blurRadius: 15 });
    const processed = await processor.process(mockTrack);

    expect(processed.id).toBe("processed-video-track");
    expect(processor.getMode()).toBe("blur");
    expect(processor.getOptions().blurRadius).toBe(15);
    expect(processor.isFallbackActive()).toBe(false);

    processor.dispose();
  });

  it("dynamically updates background mode and blur options", async () => {
    const processor = new VirtualBackgroundProcessor({ mode: "none" });
    await processor.process(mockTrack);

    expect(processor.getMode()).toBe("none");

    processor.setOptions({ mode: "blur", blurRadius: 20 });
    expect(processor.getMode()).toBe("blur");
    expect(processor.getOptions().blurRadius).toBe(20);

    processor.dispose();
  });

  it("triggers automatic fallback when frame processing times exceed threshold", async () => {
    const onFpsDegraded = vi.fn();
    const processor = new VirtualBackgroundProcessor({
      mode: "blur",
      blurRadius: 4, // low radius so it quickly steps down to fallback
      maxFrameProcessingMs: 1, // artificially low threshold to trigger fallback
      onFpsDegraded,
    });

    await processor.process(mockTrack);

    // Mock performance.now to simulate slow frames
    let now = 1000;
    vi.spyOn(performance, "now").mockImplementation(() => {
      now += 10; // each frame takes 10ms, which is > 1ms threshold
      return now;
    });

    // Run renderFrame multiple times to trigger consecutive slow frames
    for (let i = 0; i < 25; i++) {
      processor.renderFrame();
    }

    expect(processor.isFallbackActive()).toBe(true);
    expect(onFpsDegraded).toHaveBeenCalledWith(true);

    processor.dispose();
  });

  it("disposes resources cleanly", async () => {
    const processor = new VirtualBackgroundProcessor({ mode: "blur" });
    const processed = await processor.process(mockTrack);

    processor.dispose();
    expect(processed.stop).toHaveBeenCalled();
    expect(processor.getProcessedTrack()).toBeNull();
  });
});
