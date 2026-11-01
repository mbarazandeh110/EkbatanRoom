export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

export interface LogAttributes {
  roomId?: string;
  peerId?: string;
  traceId?: string;
  spanId?: string;
  requestId?: string;
  [key: string]: unknown;
}

export interface WebRtcQoESnapshot {
  timestamp: number;
  rttMs: number;
  packetLossRate: number; // e.g. 0.05 for 5%
  jitterMs: number;
  bitrateKbps: number;
  freezeCount: number;
  resolution?: { width: number; height: number };
  connectionStatus: string;
}

export interface MetricDefinition {
  name: string;
  value: number;
  labels?: Record<string, string>; // Strictly guarded against high-cardinality IDs!
}

export interface TelemetryPort {
  log(level: LogLevel, eventName: string, attributes?: LogAttributes): void;
  startSpan(operationName: string, attributes?: LogAttributes): {
    end: (extraAttributes?: LogAttributes) => void;
    recordException: (error: Error) => void;
  };
  recordMetric(metric: MetricDefinition): void;
  recordQoEMetric(snapshot: WebRtcQoESnapshot): void;
}
