import { MediaDevicesPort } from "../../ports/media_devices_port";
import { DeviceInfo, MediaConstraints } from "../../domain/models/media";
import {
  MediaPermissionError,
  DeviceNotFoundError,
} from "../../domain/errors";

interface WakeLockSentinelLike {
  release: () => Promise<void>;
  addEventListener: (type: string, listener: () => void) => void;
}

interface NavigatorWithWakeLock {
  wakeLock?: {
    request: (type: string) => Promise<WakeLockSentinelLike>;
  };
}

interface WindowWithAudioContext {
  webkitAudioContext?: typeof AudioContext;
}

export class BrowserMediaAdapter implements MediaDevicesPort {
  private activeWakeLock: WakeLockSentinelLike | null = null;
  private audioContext: AudioContext | null = null;

  public async enumerateDevices(): Promise<DeviceInfo[]> {
    if (typeof navigator === "undefined" || !navigator.mediaDevices?.enumerateDevices) {
      return [];
    }
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      return devices.map((d) => ({
        deviceId: d.deviceId,
        label: d.label || `${d.kind} (${d.deviceId.substring(0, 5)})`,
        kind: d.kind,
        groupId: d.groupId,
      }));
    } catch {
      return [];
    }
  }

  public async getUserMedia(constraints: MediaConstraints): Promise<MediaStream> {
    if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) {
      throw new MediaPermissionError("Media devices API not available");
    }

    const audioConstraints: boolean | MediaTrackConstraints = constraints.audioDeviceId
      ? {
          deviceId: { exact: constraints.audioDeviceId },
          echoCancellation: constraints.echoCancellation ?? true,
          noiseSuppression: constraints.noiseSuppression ?? true,
          autoGainControl: constraints.autoGainControl ?? true,
        }
      : {
          echoCancellation: constraints.echoCancellation ?? true,
          noiseSuppression: constraints.noiseSuppression ?? true,
          autoGainControl: constraints.autoGainControl ?? true,
        };

    const videoConstraints: boolean | MediaTrackConstraints = constraints.videoDeviceId
      ? {
          deviceId: { exact: constraints.videoDeviceId },
          width: constraints.resolution?.width ? { ideal: constraints.resolution.width } : undefined,
          height: constraints.resolution?.height ? { ideal: constraints.resolution.height } : undefined,
          frameRate: constraints.resolution?.frameRate ? { ideal: constraints.resolution.frameRate } : undefined,
        }
      : constraints.resolution
      ? {
          width: { ideal: constraints.resolution.width },
          height: { ideal: constraints.resolution.height },
          frameRate: constraints.resolution.frameRate ? { ideal: constraints.resolution.frameRate } : undefined,
        }
      : true;

    try {
      return await navigator.mediaDevices.getUserMedia({
        audio: audioConstraints,
        video: videoConstraints,
      });
    } catch (err: unknown) {
      const errName = err instanceof Error ? err.name : "";
      if (errName === "NotAllowedError" || errName === "PermissionDeniedError") {
        throw new MediaPermissionError("Permission to access camera or microphone was denied");
      }
      if (errName === "NotFoundError" || errName === "DevicesNotFoundError") {
        throw new DeviceNotFoundError("Requested camera or microphone device not found");
      }
      throw err;
    }
  }

  public async getDisplayMedia(): Promise<MediaStream> {
    if (typeof navigator === "undefined" || !navigator.mediaDevices?.getDisplayMedia) {
      throw new Error("Screen sharing not supported on this device/browser");
    }
    return await navigator.mediaDevices.getDisplayMedia({
      video: true,
      audio: false,
    });
  }

  public async unlockAudioContext(): Promise<boolean> {
    if (typeof window === "undefined") return false;
    const AudioCtx =
      window.AudioContext ||
      (window as unknown as WindowWithAudioContext).webkitAudioContext;
    if (!AudioCtx) return false;

    if (!this.audioContext || this.audioContext.state === "closed") {
      this.audioContext = new AudioCtx();
    }

    if (this.audioContext.state === "suspended") {
      try {
        await this.audioContext.resume();
        return (this.audioContext.state as AudioContextState) === "running";
      } catch {
        return false;
      }
    }
    return (this.audioContext.state as AudioContextState) === "running";
  }

  public async requestWakeLock(): Promise<boolean> {
    if (typeof navigator === "undefined" || !("wakeLock" in navigator)) {
      return false;
    }
    try {
      const nav = navigator as unknown as NavigatorWithWakeLock;
      if (nav.wakeLock?.request) {
        this.activeWakeLock = await nav.wakeLock.request("screen");
        this.activeWakeLock.addEventListener("release", () => {
          this.activeWakeLock = null;
        });
        return true;
      }
      return false;
    } catch {
      return false;
    }
  }

  public async releaseWakeLock(): Promise<void> {
    if (this.activeWakeLock) {
      try {
        await this.activeWakeLock.release();
      } catch {}
      this.activeWakeLock = null;
    }
  }

  public stopStream(stream: MediaStream): void {
    stream.getTracks().forEach((track) => {
      try {
        track.stop();
      } catch {}
    });
  }
}
