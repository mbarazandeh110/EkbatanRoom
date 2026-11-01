export class DomainError extends Error {
  constructor(message: string, public readonly code: string) {
    super(message);
    this.name = "DomainError";
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export class MediaPermissionError extends DomainError {
  constructor(message: string = "Permission to access camera or microphone was denied") {
    super(message, "MEDIA_PERMISSION_DENIED");
    this.name = "MediaPermissionError";
  }
}

export class DeviceNotFoundError extends DomainError {
  constructor(message: string = "Requested media device was not found") {
    super(message, "DEVICE_NOT_FOUND");
    this.name = "DeviceNotFoundError";
  }
}

export class SignalingError extends DomainError {
  constructor(message: string, public readonly statusCode?: number) {
    super(message, "SIGNALING_ERROR");
    this.name = "SignalingError";
  }
}

export class RtcEngineError extends DomainError {
  constructor(message: string, public readonly originalError?: unknown) {
    super(message, "RTC_ENGINE_ERROR");
    this.name = "RtcEngineError";
  }
}

export class AudioContextBlockedError extends DomainError {
  constructor(message: string = "Audio playback was blocked by browser autoplay policy") {
    super(message, "AUDIO_CONTEXT_BLOCKED");
    this.name = "AudioContextBlockedError";
  }
}
