import {
  LogLevel,
  LogAttributes,
  WebRtcQoESnapshot,
  MetricDefinition,
  TelemetryPort,
} from "../../ports/telemetry_port";

const LOG_LEVEL_SEVERITY: Record<LogLevel, number> = {
  trace: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
};

const FORBIDDEN_METRIC_LABELS = new Set([
  "room_id",
  "roomid",
  "peer_id",
  "peerid",
  "transport_id",
  "transportid",
  "request_id",
  "requestid",
  "user_id",
  "userid",
]);

const SENSITIVE_KEYS = new Set([
  "token",
  "access_token",
  "accesstoken",
  "authorization",
  "password",
  "ice_pwd",
  "icepwd",
  "secret",
  "jwt",
]);

export interface TelemetryConfig {
  logLevel?: LogLevel;
  logFormat?: "json" | "pretty";
  logRawSdp?: boolean;
  metricsEnabled?: boolean;
  tracingEnabled?: boolean;
  sampleRate?: number;
}

export class ConfigurableTelemetryAdapter implements TelemetryPort {
  private config: Required<TelemetryConfig>;
  private lastQoEAnomalyLogTimestamp: number = 0;

  constructor(customConfig?: TelemetryConfig) {
    const env: Record<string, string | undefined> =
      typeof process !== "undefined" ? process.env : {};

    const envLogLevel = (
      env.OBS_LOG_LEVEL ||
      env.NEXT_PUBLIC_OBS_LOG_LEVEL ||
      "info"
    ).toLowerCase() as LogLevel;

    const envLogFormat = (
      env.OBS_LOG_FORMAT ||
      env.NEXT_PUBLIC_OBS_LOG_FORMAT ||
      "json"
    ).toLowerCase() as "json" | "pretty";

    const envRawSdp =
      (env.OBS_LOG_RAW_SDP || env.NEXT_PUBLIC_OBS_LOG_RAW_SDP) === "true";

    const envMetricsEnabled =
      (env.OBS_METRICS_ENABLED || env.NEXT_PUBLIC_OBS_METRICS_ENABLED) !== "false";

    const envTracingEnabled =
      (env.OBS_TRACING_ENABLED || env.NEXT_PUBLIC_OBS_TRACING_ENABLED) === "true";

    const envSampleRate = parseFloat(
      env.OBS_TRACE_SAMPLE_RATE ||
        env.NEXT_PUBLIC_OBS_TRACE_SAMPLE_RATE ||
        "0.1"
    );

    this.config = {
      logLevel: customConfig?.logLevel ?? envLogLevel,
      logFormat: customConfig?.logFormat ?? envLogFormat,
      logRawSdp: customConfig?.logRawSdp ?? envRawSdp,
      metricsEnabled: customConfig?.metricsEnabled ?? envMetricsEnabled,
      tracingEnabled: customConfig?.tracingEnabled ?? envTracingEnabled,
      sampleRate: customConfig?.sampleRate ?? (isNaN(envSampleRate) ? 0.1 : envSampleRate),
    };
  }

  public sanitizeAttributes(
    attributes?: LogAttributes,
    currentLevel: LogLevel = "info"
  ): Record<string, unknown> {
    if (!attributes) return {};
    const sanitized: Record<string, unknown> = {};

    for (const [key, val] of Object.entries(attributes)) {
      const lowerKey = key.toLowerCase();

      // Check for sensitive keys
      if (lowerKey === "authorization") {
        if (typeof val === "string" && /^bearer\s+/i.test(val)) {
          sanitized[key] = "Bearer [REDACTED]";
        } else {
          sanitized[key] = "[REDACTED]";
        }
        continue;
      }

      if (SENSITIVE_KEYS.has(lowerKey)) {
        sanitized[key] = "[REDACTED]";
        continue;
      }

      // Check for SDP
      if (lowerKey === "sdp" || lowerKey === "raw_sdp") {
        if (this.config.logRawSdp && currentLevel === "debug") {
          sanitized[key] = val;
        } else {
          sanitized[key] = "[REDACTED_SDP]";
        }
        continue;
      }

      // Check string contents for JWT or Bearer tokens
      if (typeof val === "string") {
        if (/Bearer\s+[A-Za-z0-9-_=]+\.[A-Za-z0-9-_=]+\.?[A-Za-z0-9-_.+/=]*/i.test(val)) {
          sanitized[key] = val.replace(/Bearer\s+[^\s]+/gi, "Bearer [REDACTED]");
          continue;
        }
        if (/^eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+$/.test(val)) {
          sanitized[key] = "[REDACTED_JWT]";
          continue;
        }
      }

      // Recursively sanitize nested objects
      if (val && typeof val === "object" && !Array.isArray(val) && !(val instanceof Error)) {
        sanitized[key] = this.sanitizeAttributes(val as LogAttributes, currentLevel);
        continue;
      }

      sanitized[key] = val;
    }

    return sanitized;
  }

  public log(level: LogLevel, eventName: string, attributes?: LogAttributes): void {
    if (LOG_LEVEL_SEVERITY[level] < LOG_LEVEL_SEVERITY[this.config.logLevel]) {
      return;
    }

    // Media hot-path protection: drop per-RTP-packet or unthrottled audio level logs
    if (eventName === "rtp_packet" || eventName === "unthrottled_audio_level") {
      return;
    }

    const sanitized = this.sanitizeAttributes(attributes, level);
    const logEntry = {
      timestamp: new Date().toISOString(),
      level,
      event: eventName,
      ...sanitized,
    };

    if (this.config.logFormat === "pretty") {
      const meta = Object.keys(sanitized).length > 0 ? sanitized : "";
      if (level === "error") {
        console.error(`[${level.toUpperCase()}] ${eventName}`, meta);
      } else if (level === "warn") {
        console.warn(`[${level.toUpperCase()}] ${eventName}`, meta);
      } else {
        console.log(`[${level.toUpperCase()}] ${eventName}`, meta);
      }
    } else {
      const json = JSON.stringify(logEntry);
      if (level === "error") {
        console.error(json);
      } else if (level === "warn") {
        console.warn(json);
      } else {
        console.log(json);
      }
    }
  }

  public startSpan(operationName: string, attributes?: LogAttributes) {
    const shouldSample =
      this.config.tracingEnabled && Math.random() <= this.config.sampleRate;
    const startTime = Date.now();
    const traceId = attributes?.traceId || `trace-${Math.random().toString(36).substring(2, 9)}`;
    const spanId = `span-${Math.random().toString(36).substring(2, 9)}`;

    if (shouldSample) {
      this.log("debug", `span_start:${operationName}`, {
        ...attributes,
        traceId,
        spanId,
      });
    }

    return {
      end: (extraAttributes?: LogAttributes) => {
        const durationMs = Date.now() - startTime;
        if (shouldSample) {
          this.log("debug", `span_end:${operationName}`, {
            ...attributes,
            ...extraAttributes,
            traceId,
            spanId,
            durationMs,
          });
        }
      },
      recordException: (error: Error) => {
        // Errors are always 100% recorded
        this.log("error", `span_error:${operationName}`, {
          ...attributes,
          traceId,
          spanId,
          errorMessage: error.message,
          errorStack: error.stack,
        });
      },
    };
  }

  public recordMetric(metric: MetricDefinition): void {
    if (!this.config.metricsEnabled) return;

    // Strict Cardinality Guard: remove high-cardinality IDs from Prometheus/VictoriaMetrics labels
    const sanitizedLabels: Record<string, string> = {};
    if (metric.labels) {
      for (const [key, value] of Object.entries(metric.labels)) {
        if (!FORBIDDEN_METRIC_LABELS.has(key.toLowerCase())) {
          sanitizedLabels[key] = value;
        }
      }
    }

    // In browser, metrics are accumulated or sent via telemetry endpoint
    if (this.config.logLevel === "trace") {
      this.log("trace", "metric_recorded", {
        metric: metric.name,
        value: metric.value,
        labels: sanitizedLabels,
      });
    }
  }

  public recordQoEMetric(snapshot: WebRtcQoESnapshot): void {
    // Anomaly-Driven QoE reporting
    const isAnomaly =
      snapshot.packetLossRate > 0.02 || // > 2% loss
      snapshot.rttMs > 250 || // > 250ms RTT
      snapshot.freezeCount > 0 ||
      snapshot.connectionStatus === "reconnecting" ||
      snapshot.connectionStatus === "failed";

    if (isAnomaly) {
      const now = Date.now();
      // Throttle anomaly logs to at most once every 3 seconds
      if (now - this.lastQoEAnomalyLogTimestamp > 3000) {
        this.lastQoEAnomalyLogTimestamp = now;
        this.log("warn", "qoe_anomaly_detected", {
          rttMs: snapshot.rttMs,
          packetLossRate: snapshot.packetLossRate,
          freezeCount: snapshot.freezeCount,
          jitterMs: snapshot.jitterMs,
          status: snapshot.connectionStatus,
        });
      }
    }

    this.recordMetric({
      name: "webrtc_rtt_ms",
      value: snapshot.rttMs,
      labels: { status: snapshot.connectionStatus },
    });

    this.recordMetric({
      name: "webrtc_packet_loss_rate",
      value: snapshot.packetLossRate,
      labels: { status: snapshot.connectionStatus },
    });
  }
}
