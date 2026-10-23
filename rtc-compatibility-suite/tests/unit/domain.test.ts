import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { loadSuiteConfig } from '../../src/domain/config.js';
import type { GateStatus, GateId } from '../../src/domain/types.js';

describe('Domain Models & Config', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv };
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it('validates all required gate statuses', () => {
    const validStatuses: GateStatus[] = ['PASSED', 'FAILED', 'BLOCKED', 'NOT_RUN'];
    expect(validStatuses).toHaveLength(4);

    const validGateIds: GateId[] = [
      'gate_01_connection',
      'gate_02_bidirectional_rtp',
      'gate_03_concurrency_glare',
      'gate_04_simulcast_layers',
      'gate_05_ice_restart'
    ];
    expect(validGateIds).toHaveLength(5);
  });

  it('loads default observability configuration correctly', () => {
    delete process.env.OBS_LOG_LEVEL;
    delete process.env.OBS_LOG_FORMAT;
    delete process.env.OBS_LOG_RAW_SDP;
    delete process.env.OBS_METRICS_ENABLED;

    const config = loadSuiteConfig();
    expect(config.observability.logLevel).toBe('info');
    expect(config.observability.logFormat).toBe('json');
    expect(config.observability.logRawSdp).toBe(false);
    expect(config.observability.metricsEnabled).toBe(true);
    expect(config.signalingUrl).toBe('ws://127.0.0.1:7880');
    expect(config.sfuGrpcAddr).toBe('127.0.0.1:50051');
  });

  it('respects OBS_* environment overrides', () => {
    process.env.OBS_LOG_LEVEL = 'debug';
    process.env.OBS_LOG_FORMAT = 'pretty';
    process.env.OBS_LOG_RAW_SDP = 'true';
    process.env.OBS_CAPTURE_BROWSER_LOGS = 'true';
    process.env.OBS_METRICS_ENABLED = 'false';

    const config = loadSuiteConfig();
    expect(config.observability.logLevel).toBe('debug');
    expect(config.observability.logFormat).toBe('pretty');
    expect(config.observability.logRawSdp).toBe(true);
    expect(config.observability.captureBrowserLogs).toBe(true);
    expect(config.observability.metricsEnabled).toBe(false);
  });
});
