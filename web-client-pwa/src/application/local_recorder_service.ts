/**
 * Client-Side Local Recording & Cloud Recording Trigger Service.
 *
 * Records session media locally in the browser using the MediaRecorder API
 * with direct WebM/MP4 download, and coordinates server-side S3 cloud recording triggers.
 */

export type RecordingState = "idle" | "recording" | "paused";

export interface LocalRecordingOptions {
  mimeType?: string;
  videoBitsPerSecond?: number; // default: 2_500_000 (2.5 Mbps)
  audioBitsPerSecond?: number; // default: 128_000 (128 kbps)
  timeslice?: number;          // default: 1000 ms chunks
}

export interface CloudRecordingResponse {
  success: boolean;
  recordingId?: string;
  status: "started" | "stopped" | "failed";
  error?: string;
}

export interface RecorderEventListeners {
  stateChange?: (state: RecordingState) => void;
  durationChange?: (durationMs: number) => void;
  error?: (err: Error) => void;
}

export class LocalRecorderService {
  private mediaRecorder: MediaRecorder | null = null;
  private recordedChunks: Blob[] = [];
  private state: RecordingState = "idle";
  private startTime: number = 0;
  private pausedDuration: number = 0;
  private pauseStartTime: number = 0;
  private timerInterval: ReturnType<typeof setInterval> | null = null;
  private latestRecordedBlob: Blob | null = null;
  private listeners: RecorderEventListeners = {};

  constructor(listeners?: RecorderEventListeners) {
    if (listeners) {
      this.listeners = listeners;
    }
  }

  public getState(): RecordingState {
    return this.state;
  }

  public getDurationMs(): number {
    if (this.state === "idle" || this.startTime === 0) return 0;
    if (this.state === "paused") {
      return this.pauseStartTime - this.startTime - this.pausedDuration;
    }
    return Date.now() - this.startTime - this.pausedDuration;
  }

  public getLatestBlob(): Blob | null {
    return this.latestRecordedBlob;
  }

  public setEventListeners(listeners: RecorderEventListeners): void {
    this.listeners = { ...this.listeners, ...listeners };
  }

  private setState(nextState: RecordingState): void {
    this.state = nextState;
    this.listeners.stateChange?.(nextState);
  }

  public static getSupportedMimeType(): string {
    if (typeof MediaRecorder === "undefined") {
      return "video/webm";
    }

    const candidateTypes = [
      "video/webm;codecs=vp9,opus",
      "video/webm;codecs=vp8,opus",
      "video/webm",
      "video/mp4;codecs=avc1,mp4a.40.2",
      "video/mp4",
    ];

    for (const type of candidateTypes) {
      if (typeof MediaRecorder.isTypeSupported === "function" && MediaRecorder.isTypeSupported(type)) {
        return type;
      }
    }

    return "video/webm";
  }

  public async startRecording(
    stream: MediaStream,
    options?: LocalRecordingOptions
  ): Promise<void> {
    if (this.state !== "idle") {
      throw new Error(`Cannot start recording while in '${this.state}' state`);
    }

    if (typeof MediaRecorder === "undefined") {
      throw new Error("MediaRecorder API is not supported in this browser");
    }

    if (!stream || stream.getTracks().length === 0) {
      throw new Error("Cannot record empty MediaStream");
    }

    this.recordedChunks = [];
    this.latestRecordedBlob = null;

    const mimeType = options?.mimeType || LocalRecorderService.getSupportedMimeType();
    const recorderOptions: MediaRecorderOptions = {
      mimeType,
      videoBitsPerSecond: options?.videoBitsPerSecond ?? 2_500_000,
      audioBitsPerSecond: options?.audioBitsPerSecond ?? 128_000,
    };

    try {
      this.mediaRecorder = new MediaRecorder(stream, recorderOptions);
    } catch {
      // Fallback without explicit bitrate/mimetype if unsupported
      this.mediaRecorder = new MediaRecorder(stream);
    }

    this.mediaRecorder.ondataavailable = (event: BlobEvent) => {
      if (event.data && event.data.size > 0) {
        this.recordedChunks.push(event.data);
      }
    };

    this.mediaRecorder.onerror = (event: Event) => {
      const err = new Error(`MediaRecorder error: ${(event as unknown as { error?: Error })?.error?.message || "Unknown error"}`);
      this.listeners.error?.(err);
    };

    this.startTime = Date.now();
    this.pausedDuration = 0;
    this.pauseStartTime = 0;

    const timeslice = options?.timeslice ?? 1000;
    this.mediaRecorder.start(timeslice);
    this.setState("recording");

    this.timerInterval = setInterval(() => {
      if (this.state === "recording") {
        this.listeners.durationChange?.(this.getDurationMs());
      }
    }, 500);
  }

  public pauseRecording(): void {
    if (this.state !== "recording" || !this.mediaRecorder) return;
    try {
      this.mediaRecorder.pause();
      this.pauseStartTime = Date.now();
      this.setState("paused");
    } catch (err) {
      this.listeners.error?.(err instanceof Error ? err : new Error(String(err)));
    }
  }

  public resumeRecording(): void {
    if (this.state !== "paused" || !this.mediaRecorder) return;
    try {
      this.mediaRecorder.resume();
      this.pausedDuration += Date.now() - this.pauseStartTime;
      this.pauseStartTime = 0;
      this.setState("recording");
    } catch (err) {
      this.listeners.error?.(err instanceof Error ? err : new Error(String(err)));
    }
  }

  public async stopRecording(): Promise<Blob> {
    if (this.state === "idle" || !this.mediaRecorder) {
      if (this.latestRecordedBlob) return this.latestRecordedBlob;
      throw new Error("No active recording to stop");
    }

    return new Promise<Blob>((resolve, reject) => {
      const recorder = this.mediaRecorder!;

      const handleStop = () => {
        if (this.timerInterval) {
          clearInterval(this.timerInterval);
          this.timerInterval = null;
        }

        const mimeType = recorder.mimeType || "video/webm";
        const recordedBlob = new Blob(this.recordedChunks, { type: mimeType });
        this.latestRecordedBlob = recordedBlob;
        this.setState("idle");
        this.mediaRecorder = null;
        resolve(recordedBlob);
      };

      recorder.onstop = handleStop;

      try {
        recorder.stop();
      } catch (err) {
        reject(err instanceof Error ? err : new Error(String(err)));
      }
    });
  }

  public downloadRecording(filename?: string): void {
    if (!this.latestRecordedBlob && this.recordedChunks.length === 0) {
      throw new Error("No recorded media available for download");
    }

    const blob = this.latestRecordedBlob || new Blob(this.recordedChunks, { type: "video/webm" });
    const isMp4 = blob.type.includes("mp4");
    const defaultFilename = `recording-${new Date().toISOString().replace(/[:.]/g, "-")}.${isMp4 ? "mp4" : "webm"}`;
    const finalFilename = filename || defaultFilename;

    if (typeof window === "undefined" || typeof document === "undefined") return;

    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.style.display = "none";
    a.href = url;
    a.download = finalFilename;
    document.body.appendChild(a);
    a.click();

    setTimeout(() => {
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    }, 100);
  }

  public async triggerCloudRecording(
    roomId: string,
    action: "start" | "stop"
  ): Promise<CloudRecordingResponse> {
    try {
      const response = await fetch("/api/recording", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ roomId, action }),
      });

      if (!response.ok) {
        const errorData = await response.json().catch(() => ({}));
        return {
          success: false,
          status: "failed",
          error: (errorData as { error?: string })?.error || `HTTP ${response.status}`,
        };
      }

      const data = await response.json();
      return {
        success: true,
        recordingId: (data as { recordingId?: string })?.recordingId,
        status: action === "start" ? "started" : "stopped",
      };
    } catch (err) {
      return {
        success: false,
        status: "failed",
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }
}
