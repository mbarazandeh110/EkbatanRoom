export interface ObservabilityConfig {
  logLevel: 'trace' | 'debug' | 'info' | 'warn' | 'error';
  logFormat: 'json' | 'pretty';
  logRawSdp: boolean;
  metricsEnabled: boolean;
  captureBrowserLogs: boolean;
  tracingEnabled: boolean;
  otlpEndpoint?: string;
  traceSampleRate: number;
}

export interface SuiteConfig {
  signalingUrl: string;
  sfuGrpcAddr: string;
  harnessUrl: string;
  reportPath: string;
  observability: ObservabilityConfig;
}

export function loadSuiteConfig(): SuiteConfig {
  const env = process.env;

  const rawLogLevel = (env.OBS_LOG_LEVEL || 'info').toLowerCase();
  const validLogLevels = ['trace', 'debug', 'info', 'warn', 'error'] as const;
  const logLevel = (validLogLevels.includes(rawLogLevel as typeof validLogLevels[number])
    ? rawLogLevel
    : 'info') as ObservabilityConfig['logLevel'];

  const observability: ObservabilityConfig = {
    logLevel,
    logFormat: env.OBS_LOG_FORMAT === 'pretty' ? 'pretty' : 'json',
    logRawSdp: env.OBS_LOG_RAW_SDP === 'true',
    metricsEnabled: env.OBS_METRICS_ENABLED !== 'false',
    captureBrowserLogs: env.OBS_CAPTURE_BROWSER_LOGS === 'true',
    tracingEnabled: env.OBS_TRACING_ENABLED === 'true',
    otlpEndpoint: env.OBS_OTLP_ENDPOINT || undefined,
    traceSampleRate: parseFloat(env.OBS_TRACE_SAMPLE_RATE || '1.0') || 1.0
  };

  return {
    signalingUrl: env.SIGNALING_URL || 'ws://127.0.0.1:7880',
    sfuGrpcAddr: env.SFU_GRPC_ADDR || '127.0.0.1:50051',
    harnessUrl: env.HARNESS_URL || 'http://localhost:5173',
    reportPath: env.EVIDENCE_REPORT_PATH || 'test-results/evidence-report.json',
    observability
  };
}
