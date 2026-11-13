import { describe, it, expect, vi, beforeEach } from "vitest";
import { LocalRecorderService } from "../local_recorder_service";

interface MockRecorder {
  state: string;
  mimeType: string;
  start: (timeslice?: number) => void;
  pause: () => void;
  resume: () => void;
  stop: () => void;
  ondataavailable: ((event: { data: Blob }) => void) | null;
  onstop: (() => void) | null;
  onerror: ((err: unknown) => void) | null;
}

describe("LocalRecorderService", () => {
  let mockStream: MediaStream;
  let mockTrack: MediaStreamTrack;
  let mockRecorderInstance: MockRecorder;

  beforeEach(() => {
    mockTrack = {
      kind: "video",
      id: "v-1",
      stop: vi.fn(),
    } as unknown as MediaStreamTrack;

    mockStream = {
      getTracks: vi.fn().mockReturnValue([mockTrack]),
    } as unknown as MediaStream;

    mockRecorderInstance = {
      state: "inactive",
      mimeType: "video/webm",
      start: vi.fn().mockImplementation(function (this: MockRecorder) {
        this.state = "recording";
      }),
      pause: vi.fn().mockImplementation(function (this: MockRecorder) {
        this.state = "paused";
      }),
      resume: vi.fn().mockImplementation(function (this: MockRecorder) {
        this.state = "recording";
      }),
      stop: vi.fn().mockImplementation(function (this: MockRecorder) {
        this.state = "inactive";
        if (this.ondataavailable) {
          this.ondataavailable({ data: new Blob(["sample-data"], { type: "video/webm" }) });
        }
        if (this.onstop) {
          this.onstop();
        }
      }),
      ondataavailable: null,
      onstop: null,
      onerror: null,
    };

    const MockMediaRecorder = vi.fn().mockImplementation(() => mockRecorderInstance) as unknown as {
      isTypeSupported: (type: string) => boolean;
      new (stream: MediaStream, options?: MediaRecorderOptions): MediaRecorder;
    };
    MockMediaRecorder.isTypeSupported = vi.fn().mockReturnValue(true);

    const glob = globalThis as unknown as Record<string, unknown>;
    const win = window as unknown as Record<string, unknown>;
    glob.MediaRecorder = MockMediaRecorder;
    win.MediaRecorder = MockMediaRecorder;
  });

  it("checks supported MIME types", () => {
    const mime = LocalRecorderService.getSupportedMimeType();
    expect(mime).toContain("video/webm");
  });

  it("starts recording and updates state", async () => {
    const onStateChange = vi.fn();
    const service = new LocalRecorderService({ stateChange: onStateChange });

    expect(service.getState()).toBe("idle");
    await service.startRecording(mockStream);

    expect(mockRecorderInstance.start).toHaveBeenCalledWith(1000);
    expect(service.getState()).toBe("recording");
    expect(onStateChange).toHaveBeenCalledWith("recording");
  });

  it("pauses and resumes recording", async () => {
    const service = new LocalRecorderService();
    await service.startRecording(mockStream);

    service.pauseRecording();
    expect(mockRecorderInstance.pause).toHaveBeenCalled();
    expect(service.getState()).toBe("paused");

    service.resumeRecording();
    expect(mockRecorderInstance.resume).toHaveBeenCalled();
    expect(service.getState()).toBe("recording");
  });

  it("stops recording and returns the aggregated media Blob", async () => {
    const service = new LocalRecorderService();
    await service.startRecording(mockStream);

    const blob = await service.stopRecording();
    expect(mockRecorderInstance.stop).toHaveBeenCalled();
    expect(blob).toBeInstanceOf(Blob);
    expect(service.getState()).toBe("idle");
    expect(service.getLatestBlob()).toBe(blob);
  });

  it("triggers cloud recording via fetch API", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce({
      ok: true,
      json: async () => ({ recordingId: "rec-test-123", status: "started" }),
    } as Response);

    const service = new LocalRecorderService();
    const result = await service.triggerCloudRecording("room-abc", "start");

    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/recording",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ roomId: "room-abc", action: "start" }),
      })
    );
    expect(result.success).toBe(true);
    expect(result.recordingId).toBe("rec-test-123");
    expect(result.status).toBe("started");
  });
});
