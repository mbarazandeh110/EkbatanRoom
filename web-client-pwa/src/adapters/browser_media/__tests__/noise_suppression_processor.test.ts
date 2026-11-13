import { describe, it, expect, vi, beforeEach } from "vitest";
import { NoiseSuppressionProcessor } from "../noise_suppression_processor";

interface MockNode {
  connect: ReturnType<typeof vi.fn>;
  disconnect: ReturnType<typeof vi.fn>;
}

interface MockFilter extends MockNode {
  type: string;
  frequency: { setValueAtTime: ReturnType<typeof vi.fn> };
}

interface MockCompressor extends MockNode {
  threshold: { setValueAtTime: ReturnType<typeof vi.fn> };
  knee: { setValueAtTime: ReturnType<typeof vi.fn> };
  ratio: { setValueAtTime: ReturnType<typeof vi.fn> };
  attack: { setValueAtTime: ReturnType<typeof vi.fn> };
  release: { setValueAtTime: ReturnType<typeof vi.fn> };
}

interface MockDestination {
  stream: { getAudioTracks: ReturnType<typeof vi.fn> };
  disconnect: ReturnType<typeof vi.fn>;
}

interface MockContext {
  state: string;
  currentTime: number;
  resume: ReturnType<typeof vi.fn>;
  close: ReturnType<typeof vi.fn>;
  createMediaStreamSource: ReturnType<typeof vi.fn>;
  createBiquadFilter: ReturnType<typeof vi.fn>;
  createDynamicsCompressor: ReturnType<typeof vi.fn>;
  createMediaStreamDestination: ReturnType<typeof vi.fn>;
}

describe("NoiseSuppressionProcessor", () => {
  let mockAudioContext: MockContext;
  let mockSourceNode: MockNode;
  let mockHighpass: MockFilter;
  let mockLowpass: MockFilter;
  let mockCompressor: MockCompressor;
  let mockDestination: MockDestination;
  let mockTrack: MediaStreamTrack;

  beforeEach(() => {
    mockTrack = {
      kind: "audio",
      id: "audio-track-1",
      stop: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    } as unknown as MediaStreamTrack;

    mockSourceNode = {
      connect: vi.fn(),
      disconnect: vi.fn(),
    };

    mockHighpass = {
      type: "highpass",
      frequency: { setValueAtTime: vi.fn() },
      connect: vi.fn(),
      disconnect: vi.fn(),
    };

    mockLowpass = {
      type: "lowpass",
      frequency: { setValueAtTime: vi.fn() },
      connect: vi.fn(),
      disconnect: vi.fn(),
    };

    mockCompressor = {
      threshold: { setValueAtTime: vi.fn() },
      knee: { setValueAtTime: vi.fn() },
      ratio: { setValueAtTime: vi.fn() },
      attack: { setValueAtTime: vi.fn() },
      release: { setValueAtTime: vi.fn() },
      connect: vi.fn(),
      disconnect: vi.fn(),
    };

    const mockOutputTrack = {
      kind: "audio",
      id: "processed-audio-track",
      stop: vi.fn(),
    };

    mockDestination = {
      stream: {
        getAudioTracks: vi.fn().mockReturnValue([mockOutputTrack]),
      },
      disconnect: vi.fn(),
    };

    mockAudioContext = {
      state: "running",
      currentTime: 10,
      resume: vi.fn().mockResolvedValue(undefined),
      close: vi.fn().mockResolvedValue(undefined),
      createMediaStreamSource: vi.fn().mockReturnValue(mockSourceNode),
      createBiquadFilter: vi.fn()
        .mockReturnValueOnce(mockHighpass)
        .mockReturnValueOnce(mockLowpass),
      createDynamicsCompressor: vi.fn().mockReturnValue(mockCompressor),
      createMediaStreamDestination: vi.fn().mockReturnValue(mockDestination),
    };

    // Mock global AudioContext
    (window as unknown as Record<string, unknown>).AudioContext = vi.fn().mockImplementation(() => mockAudioContext);
  });

  it("passes through non-audio tracks unchanged", async () => {
    const processor = new NoiseSuppressionProcessor({}, mockAudioContext as unknown as AudioContext);
    const videoTrack = { kind: "video", id: "video-1" } as unknown as MediaStreamTrack;
    const result = await processor.process(videoTrack);
    expect(result).toBe(videoTrack);
  });

  it("builds the audio DSP pipeline and returns the processed audio track", async () => {
    const processor = new NoiseSuppressionProcessor(
      { highpassFrequency: 90, lowpassFrequency: 8000 },
      mockAudioContext as unknown as AudioContext
    );

    const processed = await processor.process(mockTrack);
    expect(processed.id).toBe("processed-audio-track");
    expect(mockSourceNode.connect).toHaveBeenCalledWith(mockHighpass);
    expect(mockHighpass.connect).toHaveBeenCalledWith(mockCompressor);
    expect(mockCompressor.connect).toHaveBeenCalledWith(mockLowpass);
    expect(mockLowpass.connect).toHaveBeenCalledWith(mockDestination);
    expect(processor.isEnabled()).toBe(true);
  });

  it("toggles bypass when disabled and re-enabled", async () => {
    const processor = new NoiseSuppressionProcessor({}, mockAudioContext as unknown as AudioContext);
    await processor.process(mockTrack);

    // Disable noise suppression (bypass)
    processor.setEnabled(false);
    expect(processor.isEnabled()).toBe(false);
    expect(mockSourceNode.disconnect).toHaveBeenCalled();
    expect(mockSourceNode.connect).toHaveBeenCalledWith(mockDestination);

    // Re-enable
    processor.setEnabled(true);
    expect(processor.isEnabled()).toBe(true);
    expect(mockSourceNode.connect).toHaveBeenCalledWith(mockHighpass);
  });

  it("cleans up resources and stops processed track upon dispose", async () => {
    const processor = new NoiseSuppressionProcessor({}, mockAudioContext as unknown as AudioContext);
    const processed = await processor.process(mockTrack);

    processor.dispose();
    expect(processed.stop).toHaveBeenCalled();
    expect(mockSourceNode.disconnect).toHaveBeenCalled();
    expect(processor.getProcessedTrack()).toBeNull();
  });
});
