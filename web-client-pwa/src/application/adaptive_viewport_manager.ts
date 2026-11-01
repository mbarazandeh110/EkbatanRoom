import { VideoQualityLayer } from "../domain/models/media";
import { RemoteSubscriptionPort } from "../ports/rtc_engine_port";
import { TelemetryPort } from "../ports/telemetry_port";

export interface ViewportDimensions {
  width: number;
  height: number;
}

export interface AdaptiveViewportState {
  trackId: string;
  isIntersecting: boolean;
  width: number;
  height: number;
  currentQuality?: VideoQualityLayer;
  isEnabled: boolean;
}

export class AdaptiveViewportManager {
  private viewports: Map<string, AdaptiveViewportState> = new Map();
  private debounceTimers: Map<string, ReturnType<typeof setTimeout>> = new Map();
  private debounceDelayMs: number;

  constructor(
    private subscriptionPort: RemoteSubscriptionPort,
    private telemetry?: TelemetryPort,
    debounceDelayMs: number = 300
  ) {
    this.debounceDelayMs = debounceDelayMs;
  }

  /**
   * Determine optimal quality layer from rendered element width
   */
  public static calculateQualityLayer(width: number): VideoQualityLayer {
    if (width < 280) {
      return "low";
    } else if (width <= 640) {
      return "medium";
    } else {
      return "high";
    }
  }

  /**
   * Register or update a viewport's intersection and dimensions
   */
  public onViewportChange(
    trackId: string,
    isIntersecting: boolean,
    dimensions: ViewportDimensions
  ): void {
    const existing = this.viewports.get(trackId) || {
      trackId,
      isIntersecting: true,
      width: dimensions.width,
      height: dimensions.height,
      isEnabled: true,
    };

    const targetQuality = AdaptiveViewportManager.calculateQualityLayer(
      dimensions.width
    );

    // Clear any pending debounce for this track
    const existingTimer = this.debounceTimers.get(trackId);
    if (existingTimer) {
      clearTimeout(existingTimer);
    }

    // Schedule debounced update
    const timer = setTimeout(() => {
      this.applyViewportState(trackId, isIntersecting, targetQuality, dimensions);
    }, this.debounceDelayMs);

    this.debounceTimers.set(trackId, timer);

    // Update memory representation
    this.viewports.set(trackId, {
      ...existing,
      isIntersecting,
      width: dimensions.width,
      height: dimensions.height,
    });
  }

  /**
   * Apply quality and enabled status to remote subscription port immediately
   */
  public async applyViewportState(
    trackId: string,
    isIntersecting: boolean,
    targetQuality: VideoQualityLayer,
    dimensions?: ViewportDimensions
  ): Promise<void> {
    const state = this.viewports.get(trackId);

    if (!isIntersecting) {
      // Off-screen: disable track to conserve bandwidth & decoding power
      if (!state || state.isEnabled) {
        try {
          await this.subscriptionPort.setTrackEnabled(trackId, false);
          if (state) state.isEnabled = false;
          this.telemetry?.log("debug", "adaptive_stream_disabled", {
            trackId,
            reason: "off_screen",
          });
        } catch (error) {
          this.telemetry?.log("warn", "adaptive_stream_disable_failed", {
            trackId,
            error: String(error),
          });
        }
      }
      return;
    }

    // On-screen: ensure track is enabled
    if (state && !state.isEnabled) {
      try {
        await this.subscriptionPort.setTrackEnabled(trackId, true);
        state.isEnabled = true;
      } catch (error) {
        this.telemetry?.log("warn", "adaptive_stream_enable_failed", {
          trackId,
          error: String(error),
        });
      }
    }

    // Adjust quality if changed
    if (!state || state.currentQuality !== targetQuality) {
      try {
        await this.subscriptionPort.setVideoQuality(trackId, targetQuality);
        if (state) state.currentQuality = targetQuality;
        this.telemetry?.log("debug", "adaptive_stream_quality_changed", {
          trackId,
          targetQuality,
          width: dimensions?.width,
        });
      } catch (error) {
        this.telemetry?.log("warn", "adaptive_stream_quality_failed", {
          trackId,
          error: String(error),
        });
      }
    }
  }

  /**
   * Clean up track on unmount
   */
  public unregisterTrack(trackId: string): void {
    const timer = this.debounceTimers.get(trackId);
    if (timer) {
      clearTimeout(timer);
      this.debounceTimers.delete(trackId);
    }
    this.viewports.delete(trackId);
  }

  public getTrackState(trackId: string): AdaptiveViewportState | undefined {
    return this.viewports.get(trackId);
  }
}
