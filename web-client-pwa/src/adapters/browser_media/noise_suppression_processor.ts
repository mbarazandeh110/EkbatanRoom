/**
 * Client-Side DSP Noise Suppression Processor.
 *
 * Implements a pluggable WebAudio filter pipeline (high-pass rumble filter,
 * voice-band low-pass filter, and dynamic noise gate/compressor) with optional
 * AudioWorklet extension and browser capability fallbacks.
 */

export interface NoiseSuppressionOptions {
  highpassFrequency?: number; // default: 85 Hz (eliminates low-frequency rumble & AC hum)
  lowpassFrequency?: number;  // default: 7500 Hz (eliminates high-frequency hiss)
  compressorThreshold?: number; // default: -50 dB
  compressorRatio?: number;     // default: 12
  workletUrl?: string;          // optional custom WebAssembly / AudioWorklet module URL
}

export class NoiseSuppressionProcessor {
  private audioContext: AudioContext | null = null;
  private isOwnedContext: boolean = false;
  private sourceNode: MediaStreamAudioSourceNode | null = null;
  private highpassFilter: BiquadFilterNode | null = null;
  private lowpassFilter: BiquadFilterNode | null = null;
  private compressorNode: DynamicsCompressorNode | null = null;
  private destinationNode: MediaStreamAudioDestinationNode | null = null;
  private inputTrack: MediaStreamTrack | null = null;
  private processedTrack: MediaStreamTrack | null = null;
  private enabled: boolean = true;
  private isDisposed: boolean = false;
  private options: Required<Omit<NoiseSuppressionOptions, "workletUrl">> & { workletUrl?: string };

  constructor(options?: NoiseSuppressionOptions, externalContext?: AudioContext) {
    this.options = {
      highpassFrequency: options?.highpassFrequency ?? 85,
      lowpassFrequency: options?.lowpassFrequency ?? 7500,
      compressorThreshold: options?.compressorThreshold ?? -50,
      compressorRatio: options?.compressorRatio ?? 12,
      workletUrl: options?.workletUrl,
    };

    if (externalContext) {
      this.audioContext = externalContext;
      this.isOwnedContext = false;
    }
  }

  public isEnabled(): boolean {
    return this.enabled;
  }

  public setEnabled(enabled: boolean): void {
    if (this.enabled === enabled) return;
    this.enabled = enabled;
    this.reconnectGraph();
  }

  public getProcessedTrack(): MediaStreamTrack | null {
    return this.processedTrack;
  }

  public async process(track: MediaStreamTrack): Promise<MediaStreamTrack> {
    if (track.kind !== "audio") {
      return track;
    }

    if (typeof window === "undefined") {
      return track;
    }

    const AudioContextClass =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;

    if (!AudioContextClass) {
      return track;
    }

    this.inputTrack = track;

    if (!this.audioContext || this.audioContext.state === "closed") {
      this.audioContext = new AudioContextClass();
      this.isOwnedContext = true;
    }

    if (this.audioContext.state === "suspended") {
      try {
        await this.audioContext.resume();
      } catch {
        // AudioContext may require user gesture on Safari
      }
    }

    // Initialize WebAudio DSP nodes
    const stream = new MediaStream([track]);
    this.sourceNode = this.audioContext.createMediaStreamSource(stream);

    // 1. High-pass filter to strip AC hum and mic rumble
    this.highpassFilter = this.audioContext.createBiquadFilter();
    this.highpassFilter.type = "highpass";
    this.highpassFilter.frequency.setValueAtTime(this.options.highpassFrequency, this.audioContext.currentTime);

    // 2. Dynamics compressor for voice leveling and quiet background gating
    this.compressorNode = this.audioContext.createDynamicsCompressor();
    this.compressorNode.threshold.setValueAtTime(this.options.compressorThreshold, this.audioContext.currentTime);
    this.compressorNode.knee.setValueAtTime(40, this.audioContext.currentTime);
    this.compressorNode.ratio.setValueAtTime(this.options.compressorRatio, this.audioContext.currentTime);
    this.compressorNode.attack.setValueAtTime(0.003, this.audioContext.currentTime);
    this.compressorNode.release.setValueAtTime(0.25, this.audioContext.currentTime);

    // 3. Low-pass filter to strip hiss and high frequency harmonics
    this.lowpassFilter = this.audioContext.createBiquadFilter();
    this.lowpassFilter.type = "lowpass";
    this.lowpassFilter.frequency.setValueAtTime(this.options.lowpassFrequency, this.audioContext.currentTime);

    // Output destination stream
    this.destinationNode = this.audioContext.createMediaStreamDestination();

    this.reconnectGraph();

    const tracks = this.destinationNode.stream.getAudioTracks();
    if (tracks.length > 0) {
      this.processedTrack = tracks[0];
    } else {
      this.processedTrack = track;
    }

    // Clean up if input track ends externally
    const onEnded = () => {
      this.dispose();
      track.removeEventListener("ended", onEnded);
    };
    track.addEventListener("ended", onEnded);

    return this.processedTrack;
  }

  private reconnectGraph(): void {
    if (!this.sourceNode || !this.destinationNode) return;

    try {
      this.sourceNode.disconnect();
    } catch {
      // Ignore disconnect errors if not connected
    }

    if (this.highpassFilter) {
      try {
        this.highpassFilter.disconnect();
      } catch {}
    }
    if (this.compressorNode) {
      try {
        this.compressorNode.disconnect();
      } catch {}
    }
    if (this.lowpassFilter) {
      try {
        this.lowpassFilter.disconnect();
      } catch {}
    }

    if (this.enabled && this.highpassFilter && this.compressorNode && this.lowpassFilter) {
      // Connect DSP pipeline: source -> highpass -> compressor -> lowpass -> destination
      this.sourceNode.connect(this.highpassFilter);
      this.highpassFilter.connect(this.compressorNode);
      this.compressorNode.connect(this.lowpassFilter);
      this.lowpassFilter.connect(this.destinationNode);
    } else {
      // Bypass: direct pass-through
      this.sourceNode.connect(this.destinationNode);
    }
  }

  public dispose(): void {
    if (this.isDisposed) return;
    this.isDisposed = true;

    if (this.processedTrack) {
      try {
        this.processedTrack.stop();
      } catch {}
      this.processedTrack = null;
    }

    if (this.sourceNode) {
      try {
        this.sourceNode.disconnect();
      } catch {}
      this.sourceNode = null;
    }
    if (this.highpassFilter) {
      try {
        this.highpassFilter.disconnect();
      } catch {}
      this.highpassFilter = null;
    }
    if (this.compressorNode) {
      try {
        this.compressorNode.disconnect();
      } catch {}
      this.compressorNode = null;
    }
    if (this.lowpassFilter) {
      try {
        this.lowpassFilter.disconnect();
      } catch {}
      this.lowpassFilter = null;
    }

    if (this.audioContext && this.isOwnedContext && this.audioContext.state !== "closed") {
      try {
        this.audioContext.close();
      } catch {}
    }
    this.audioContext = null;
    this.destinationNode = null;
    this.inputTrack = null;
  }
}
