import { describe, it, expect, vi, beforeEach } from "vitest";
import { ConfigurableTelemetryAdapter } from "../configurable_telemetry_adapter";

describe("ConfigurableTelemetryAdapter", () => {
  let consoleLogSpy: ReturnType<typeof vi.spyOn>;
  let consoleWarnSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    consoleLogSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    consoleWarnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
  });

  it("redacts sensitive authentication tokens and passwords", () => {
    const adapter = new ConfigurableTelemetryAdapter({
      logLevel: "info",
      logFormat: "json",
    });

    const sanitized = adapter.sanitizeAttributes({
      access_token: "secret-token-12345",
      ice_pwd: "top-secret-ice-password",
      authorization: "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.doNotLeak",
      roomId: "public-room-1",
    });

    expect(sanitized.access_token).toBe("[REDACTED]");
    expect(sanitized.ice_pwd).toBe("[REDACTED]");
    expect(sanitized.authorization).toBe("Bearer [REDACTED]");
    expect(sanitized.roomId).toBe("public-room-1");
  });

  it("redacts SDP text unless logRawSdp=true at debug level", () => {
    const adapterWithoutRawSdp = new ConfigurableTelemetryAdapter({
      logLevel: "debug",
      logRawSdp: false,
    });

    const sanitized1 = adapterWithoutRawSdp.sanitizeAttributes(
      { sdp: "v=0\r\no=- 20518 0 IN IP4 203.0.113.1" },
      "debug"
    );
    expect(sanitized1.sdp).toBe("[REDACTED_SDP]");

    const adapterWithRawSdp = new ConfigurableTelemetryAdapter({
      logLevel: "debug",
      logRawSdp: true,
    });

    const sanitized2 = adapterWithRawSdp.sanitizeAttributes(
      { sdp: "v=0\r\no=- 20518 0 IN IP4 203.0.113.1" },
      "debug"
    );
    expect(sanitized2.sdp).toBe("v=0\r\no=- 20518 0 IN IP4 203.0.113.1");
  });

  it("enforces metric cardinality guards by removing high-cardinality IDs from labels", () => {
    const adapter = new ConfigurableTelemetryAdapter({
      metricsEnabled: true,
      logLevel: "trace",
    });

    adapter.recordMetric({
      name: "connection_duration_seconds",
      value: 120,
      labels: {
        room_id: "room-abc-xyz",
        peer_id: "peer-999",
        region: "eu-west-1",
        codec: "opus",
      },
    });

    // In trace level, the sanitized labels are logged
    expect(consoleLogSpy).toHaveBeenCalled();
    const loggedJson = JSON.parse(String(consoleLogSpy.mock.calls[0][0]));
    expect(loggedJson.labels.region).toBe("eu-west-1");
    expect(loggedJson.labels.codec).toBe("opus");
    expect(loggedJson.labels.room_id).toBeUndefined();
    expect(loggedJson.labels.peer_id).toBeUndefined();
  });

  it("triggers anomaly warning when packetLossRate or RTT exceeds threshold", () => {
    const adapter = new ConfigurableTelemetryAdapter({
      logLevel: "info",
    });

    adapter.recordQoEMetric({
      timestamp: Date.now(),
      rttMs: 320, // > 250ms -> Anomaly!
      packetLossRate: 0.05, // > 2% -> Anomaly!
      jitterMs: 15,
      bitrateKbps: 1500,
      freezeCount: 1,
      connectionStatus: "connected",
    });

    expect(consoleWarnSpy).toHaveBeenCalled();
    const loggedJson = JSON.parse(String(consoleWarnSpy.mock.calls[0][0]));
    expect(loggedJson.event).toBe("qoe_anomaly_detected");
    expect(loggedJson.rttMs).toBe(320);
    expect(loggedJson.packetLossRate).toBe(0.05);
  });
});
