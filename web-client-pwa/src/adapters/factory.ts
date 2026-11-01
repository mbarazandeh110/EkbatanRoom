import { RtcEnginePort } from "../ports/rtc_engine_port";
import { TelemetryPort } from "../ports/telemetry_port";
import { LiveKitEngineAdapter } from "./rtc_livekit/livekit_adapter";
import { MediasoupOrtcEngineAdapter } from "./rtc_ortc/mediasoup_ortc_adapter";
import { MockRtcEngineAdapter } from "./mock/mock_rtc_engine_adapter";
import { ConfigurableTelemetryAdapter } from "./telemetry/configurable_telemetry_adapter";
import { BrowserMediaAdapter } from "./browser_media/browser_media_adapter";
import { MediaDevicesPort } from "../ports/media_devices_port";

export type RtcProtocol = "livekit" | "ortc" | "mock";

export function createRtcEngine(
  protocolOverride?: RtcProtocol,
  telemetry?: TelemetryPort
): RtcEnginePort {
  const envProtocol =
    (typeof process !== "undefined" &&
      (process.env.NEXT_PUBLIC_RTC_PROTOCOL || process.env.RTC_PROTOCOL)) ||
    "livekit";

  const selectedProtocol = (protocolOverride || envProtocol).toLowerCase() as RtcProtocol;

  switch (selectedProtocol) {
    case "livekit":
      return new LiveKitEngineAdapter(telemetry);
    case "ortc":
      return new MediasoupOrtcEngineAdapter(telemetry);
    case "mock":
    default:
      return new MockRtcEngineAdapter();
  }
}

export function createMediaDevicesAdapter(): MediaDevicesPort {
  return new BrowserMediaAdapter();
}

export function createTelemetryAdapter(): TelemetryPort {
  return new ConfigurableTelemetryAdapter();
}
