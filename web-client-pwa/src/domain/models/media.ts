export type TrackSource = "camera" | "microphone" | "screen";

export type TrackKind = "audio" | "video";

export type VideoQualityLayer = "low" | "medium" | "high";

export interface TrackDescriptor {
  id: string;
  kind: TrackKind;
  source: TrackSource;
  participantId: string;
  isMuted: boolean;
  isEnabled: boolean;
  quality?: VideoQualityLayer;
  mediaStreamTrack?: MediaStreamTrack;
}

export interface DeviceInfo {
  deviceId: string;
  label: string;
  kind: "audioinput" | "audiooutput" | "videoinput";
  groupId?: string;
}

export interface MediaConstraints {
  audioDeviceId?: string;
  videoDeviceId?: string;
  echoCancellation?: boolean;
  noiseSuppression?: boolean;
  autoGainControl?: boolean;
  resolution?: {
    width: number;
    height: number;
    frameRate?: number;
  };
}
