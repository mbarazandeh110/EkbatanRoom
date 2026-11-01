import { DeviceInfo, MediaConstraints } from "../domain/models/media";

export interface MediaDevicesPort {
  enumerateDevices(): Promise<DeviceInfo[]>;
  getUserMedia(constraints: MediaConstraints): Promise<MediaStream>;
  getDisplayMedia(): Promise<MediaStream>;
  unlockAudioContext(): Promise<boolean>;
  requestWakeLock(): Promise<boolean>;
  releaseWakeLock(): Promise<void>;
  stopStream(stream: MediaStream): void;
}
