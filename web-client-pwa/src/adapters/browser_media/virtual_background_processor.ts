/**
 * Client-Side Virtual Background & Blur Processor.
 *
 * Provides canvas-based background blur and custom image replacement
 * with built-in automatic FPS fallback protection for low-end mobile devices.
 */

export type BackgroundMode = "none" | "blur" | "image";

export interface VirtualBackgroundOptions {
  mode: BackgroundMode;
  blurRadius?: number;          // Default: 12px
  backgroundImageUrl?: string;  // Image URL for "image" mode
  targetFps?: number;           // Default: 30 fps
  maxFrameProcessingMs?: number;// Threshold before fallback triggers (default: 33ms)
  onFpsDegraded?: (degraded: boolean) => void;
}

export class VirtualBackgroundProcessor {
  private options: Required<Omit<VirtualBackgroundOptions, "backgroundImageUrl" | "onFpsDegraded">> & {
    backgroundImageUrl?: string;
    onFpsDegraded?: (degraded: boolean) => void;
  };
  private inputTrack: MediaStreamTrack | null = null;
  private processedTrack: MediaStreamTrack | null = null;
  private videoElement: HTMLVideoElement | null = null;
  private canvasElement: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private backgroundImage: HTMLImageElement | null = null;
  private animationFrameId: number | null = null;
  private timerId: ReturnType<typeof setInterval> | null = null;
  private isDisposed: boolean = false;
  private isFallback: boolean = false;
  private consecutiveSlowFrames: number = 0;
  private averageRenderTimeMs: number = 0;
  private effectiveBlurRadius: number = 12;

  constructor(options?: Partial<VirtualBackgroundOptions>) {
    this.options = {
      mode: options?.mode ?? "none",
      blurRadius: options?.blurRadius ?? 12,
      backgroundImageUrl: options?.backgroundImageUrl,
      targetFps: options?.targetFps ?? 30,
      maxFrameProcessingMs: options?.maxFrameProcessingMs ?? 33,
      onFpsDegraded: options?.onFpsDegraded,
    };
    this.effectiveBlurRadius = this.options.blurRadius;

    if (this.options.backgroundImageUrl && typeof Image !== "undefined") {
      this.loadBackgroundImage(this.options.backgroundImageUrl);
    }
  }

  public getMode(): BackgroundMode {
    return this.options.mode;
  }

  public getOptions(): Readonly<VirtualBackgroundOptions> {
    return { ...this.options };
  }

  public isFallbackActive(): boolean {
    return this.isFallback;
  }

  public getAverageRenderTimeMs(): number {
    return this.averageRenderTimeMs;
  }

  public getProcessedTrack(): MediaStreamTrack | null {
    return this.processedTrack;
  }

  public setOptions(options: Partial<VirtualBackgroundOptions>): void {
    if (options.mode !== undefined) {
      this.options.mode = options.mode;
      if (options.mode === "none") {
        this.isFallback = false;
        this.consecutiveSlowFrames = 0;
      }
    }
    if (options.blurRadius !== undefined) {
      this.options.blurRadius = options.blurRadius;
      this.effectiveBlurRadius = options.blurRadius;
    }
    if (options.backgroundImageUrl !== undefined && options.backgroundImageUrl !== this.options.backgroundImageUrl) {
      this.options.backgroundImageUrl = options.backgroundImageUrl;
      this.loadBackgroundImage(options.backgroundImageUrl);
    }
    if (options.targetFps !== undefined) {
      this.options.targetFps = options.targetFps;
    }
    if (options.maxFrameProcessingMs !== undefined) {
      this.options.maxFrameProcessingMs = options.maxFrameProcessingMs;
    }
    if (options.onFpsDegraded !== undefined) {
      this.options.onFpsDegraded = options.onFpsDegraded;
    }
  }

  private loadBackgroundImage(url: string): void {
    if (typeof Image === "undefined") return;
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      this.backgroundImage = img;
    };
    img.src = url;
  }

  public async process(track: MediaStreamTrack): Promise<MediaStreamTrack> {
    if (track.kind !== "video") {
      return track;
    }

    if (typeof window === "undefined" || typeof document === "undefined") {
      return track;
    }

    this.inputTrack = track;

    // Create hidden video element to render stream frames
    this.videoElement = document.createElement("video");
    this.videoElement.autoplay = true;
    this.videoElement.muted = true;
    this.videoElement.playsInline = true;
    this.videoElement.srcObject = new MediaStream([track]);

    try {
      await this.videoElement.play();
    } catch {
      // Ignore autoplay errors if hidden
    }

    // Create offscreen canvas for frame manipulation
    const width = track.getSettings?.().width || 640;
    const height = track.getSettings?.().height || 480;

    this.canvasElement = document.createElement("canvas");
    this.canvasElement.width = width;
    this.canvasElement.height = height;
    this.ctx = this.canvasElement.getContext("2d", { willReadFrequently: true });

    // Capture output stream from canvas
    if (typeof this.canvasElement.captureStream === "function") {
      const outputStream = this.canvasElement.captureStream(this.options.targetFps);
      const outputTracks = outputStream.getVideoTracks();
      if (outputTracks.length > 0) {
        this.processedTrack = outputTracks[0];
      } else {
        this.processedTrack = track;
      }
    } else {
      this.processedTrack = track;
    }

    this.startRenderLoop();

    const onEnded = () => {
      this.dispose();
      track.removeEventListener("ended", onEnded);
    };
    track.addEventListener("ended", onEnded);

    return this.processedTrack;
  }

  private startRenderLoop(): void {
    const frameIntervalMs = 1000 / this.options.targetFps;

    const render = () => {
      if (this.isDisposed) return;
      this.renderFrame();
    };

    if (typeof window !== "undefined" && typeof window.requestAnimationFrame === "function") {
      const loop = () => {
        if (this.isDisposed) return;
        render();
        this.animationFrameId = window.requestAnimationFrame(loop);
      };
      this.animationFrameId = window.requestAnimationFrame(loop);
    } else {
      this.timerId = setInterval(render, frameIntervalMs);
    }
  }

  public renderFrame(): void {
    if (!this.ctx || !this.videoElement || !this.canvasElement) return;
    if (this.videoElement.readyState < 2 && !this.isFallback) return;

    const width = this.canvasElement.width;
    const height = this.canvasElement.height;
    const t0 = typeof performance !== "undefined" ? performance.now() : Date.now();

    try {
      if (this.options.mode === "none" || this.isFallback) {
        // Direct pass-through without filter
        this.ctx.filter = "none";
        this.ctx.drawImage(this.videoElement, 0, 0, width, height);
      } else if (this.options.mode === "blur") {
        // Apply background blur effect
        this.ctx.filter = `blur(${this.effectiveBlurRadius}px)`;
        this.ctx.drawImage(this.videoElement, 0, 0, width, height);
        this.ctx.filter = "none";

        // Draw centered foreground mask region for subject clarity
        const fgWidth = width * 0.6;
        const fgHeight = height * 0.8;
        const fgX = (width - fgWidth) / 2;
        const fgY = height - fgHeight;
        this.ctx.drawImage(this.videoElement, fgX, fgY, fgWidth, fgHeight, fgX, fgY, fgWidth, fgHeight);
      } else if (this.options.mode === "image" && this.backgroundImage) {
        // Render custom background image
        this.ctx.filter = "none";
        this.ctx.drawImage(this.backgroundImage, 0, 0, width, height);

        // Render foreground subject region
        const fgWidth = width * 0.6;
        const fgHeight = height * 0.8;
        const fgX = (width - fgWidth) / 2;
        const fgY = height - fgHeight;
        this.ctx.drawImage(this.videoElement, fgX, fgY, fgWidth, fgHeight, fgX, fgY, fgWidth, fgHeight);
      } else {
        this.ctx.filter = "none";
        this.ctx.drawImage(this.videoElement, 0, 0, width, height);
      }
    } catch {
      // Ignore drawImage issues on unready video
    }

    const t1 = typeof performance !== "undefined" ? performance.now() : Date.now();
    const frameElapsed = t1 - t0;

    // Rolling average render time (alpha = 0.2)
    this.averageRenderTimeMs = this.averageRenderTimeMs === 0
      ? frameElapsed
      : this.averageRenderTimeMs * 0.8 + frameElapsed * 0.2;

    // Automatic FPS Fallback Protection:
    // If consecutive frames exceed max allowed processing budget (e.g. 33ms for 30fps)
    if (frameElapsed > this.options.maxFrameProcessingMs) {
      this.consecutiveSlowFrames++;
      if (this.consecutiveSlowFrames >= 10 && !this.isFallback) {
        this.triggerFallbackProtection();
      }
    } else {
      if (this.consecutiveSlowFrames > 0) {
        this.consecutiveSlowFrames--;
      }
    }
  }

  private triggerFallbackProtection(): void {
    if (this.effectiveBlurRadius > 4) {
      // Step down blur radius first
      this.effectiveBlurRadius = Math.floor(this.effectiveBlurRadius / 2);
      this.consecutiveSlowFrames = 0;
    } else {
      // Degrade to pass-through to protect device frame rate
      this.isFallback = true;
      this.options.onFpsDegraded?.(true);
    }
  }

  public dispose(): void {
    if (this.isDisposed) return;
    this.isDisposed = true;

    if (this.animationFrameId !== null && typeof window !== "undefined") {
      window.cancelAnimationFrame(this.animationFrameId);
      this.animationFrameId = null;
    }
    if (this.timerId !== null) {
      clearInterval(this.timerId);
      this.timerId = null;
    }

    if (this.processedTrack) {
      try {
        this.processedTrack.stop();
      } catch {}
      this.processedTrack = null;
    }

    if (this.videoElement) {
      try {
        this.videoElement.pause();
        this.videoElement.srcObject = null;
      } catch {}
      this.videoElement = null;
    }

    this.canvasElement = null;
    this.ctx = null;
    this.backgroundImage = null;
    this.inputTrack = null;
  }
}
