import { describe, it, expect } from "vitest";
import {
  createRtcEngine,
  createMediaDevicesAdapter,
  createTelemetryAdapter,
} from "../factory";
import { LiveKitEngineAdapter } from "../rtc_livekit/livekit_adapter";
import { MediasoupOrtcEngineAdapter } from "../rtc_ortc/mediasoup_ortc_adapter";
import { MockRtcEngineAdapter } from "../mock/mock_rtc_engine_adapter";
import { BrowserMediaAdapter } from "../browser_media/browser_media_adapter";
import { ConfigurableTelemetryAdapter } from "../telemetry/configurable_telemetry_adapter";

describe("Adapters Factory", () => {
  it("creates LiveKitEngineAdapter when protocol is livekit", () => {
    const engine = createRtcEngine("livekit");
    expect(engine).toBeInstanceOf(LiveKitEngineAdapter);
  });

  it("creates MediasoupOrtcEngineAdapter when protocol is ortc", () => {
    const engine = createRtcEngine("ortc");
    expect(engine).toBeInstanceOf(MediasoupOrtcEngineAdapter);
  });

  it("creates MockRtcEngineAdapter when protocol is mock", () => {
    const engine = createRtcEngine("mock");
    expect(engine).toBeInstanceOf(MockRtcEngineAdapter);
  });

  it("creates BrowserMediaAdapter and TelemetryAdapter", () => {
    expect(createMediaDevicesAdapter()).toBeInstanceOf(BrowserMediaAdapter);
    expect(createTelemetryAdapter()).toBeInstanceOf(ConfigurableTelemetryAdapter);
  });
});
