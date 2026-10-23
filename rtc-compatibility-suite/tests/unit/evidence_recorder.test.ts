import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { EvidenceRecorder, PINNED_LIVEKIT_SDK_VERSION } from '../../src/reporters/evidence_recorder.js';

describe('EvidenceRecorder', () => {
  const testReportPath = path.resolve(__dirname, '../../test-results/test-evidence-report.json');

  beforeEach(() => {
    if (fs.existsSync(testReportPath)) {
      fs.unlinkSync(testReportPath);
    }
  });

  afterEach(() => {
    if (fs.existsSync(testReportPath)) {
      fs.unlinkSync(testReportPath);
    }
  });

  it('initializes all 5 gates with NOT_RUN status', () => {
    const recorder = new EvidenceRecorder('chromium', '1148', testReportPath);
    const report = recorder.generateReport();

    expect(report.sdkVersion).toBe(PINNED_LIVEKIT_SDK_VERSION);
    expect(report.sdkVersion).toBe('2.23.0');
    expect(report.overallStatus).toBe('NOT_RUN');
    expect(report.summary.notRun).toBe(5);
    expect(report.summary.passed).toBe(0);
    expect(report.summary.failed).toBe(0);
    expect(report.summary.blocked).toBe(0);

    expect(report.gates.gate_01_connection.status).toBe('NOT_RUN');
    expect(report.gates.gate_02_bidirectional_rtp.status).toBe('NOT_RUN');
    expect(report.gates.gate_03_concurrency_glare.status).toBe('NOT_RUN');
    expect(report.gates.gate_04_simulcast_layers.status).toBe('NOT_RUN');
    expect(report.gates.gate_05_ice_restart.status).toBe('NOT_RUN');
  });

  it('records gate results and updates overall status to PARTIAL when one passes', () => {
    const recorder = new EvidenceRecorder('chromium', '1148', testReportPath);
    recorder.startGate('gate_01_connection');
    recorder.recordGateResult('gate_01_connection', 'PASSED', {
      ws_connected: {
        name: 'ws_connected',
        status: 'PASSED',
        description: 'WebSocket handshake completed'
      }
    });

    const report = recorder.generateReport();
    expect(report.overallStatus).toBe('PARTIAL');
    expect(report.summary.passed).toBe(1);
    expect(report.summary.notRun).toBe(4);
    expect(report.gates.gate_01_connection.status).toBe('PASSED');
  });

  it('propagates failure to downstream gates as BLOCKED with explicit reason', () => {
    const recorder = new EvidenceRecorder('chromium', '1148', testReportPath);
    recorder.startGate('gate_01_connection');
    recorder.recordGateResult(
      'gate_01_connection',
      'FAILED',
      {
        join_handshake: {
          name: 'join_handshake',
          status: 'FAILED',
          description: 'Join response was not received within timeout'
        }
      },
      'Signaling Join timed out after 10000ms'
    );

    recorder.markDownstreamBlocked('gate_01_connection', 'Prerequisite Gate 1 (Connection) failed');

    const report = recorder.generateReport();
    expect(report.overallStatus).toBe('FAILED');
    expect(report.summary.failed).toBe(1);
    expect(report.summary.blocked).toBe(4);
    expect(report.summary.notRun).toBe(0);

    expect(report.gates.gate_01_connection.status).toBe('FAILED');
    expect(report.gates.gate_02_bidirectional_rtp.status).toBe('BLOCKED');
    expect(report.gates.gate_02_bidirectional_rtp.blockedReason).toBe(
      'Prerequisite Gate 1 (Connection) failed'
    );
    expect(report.gates.gate_03_concurrency_glare.status).toBe('BLOCKED');
    expect(report.gates.gate_04_simulcast_layers.status).toBe('BLOCKED');
    expect(report.gates.gate_05_ice_restart.status).toBe('BLOCKED');
  });

  it('saves structured report to disk and verifies JSON structure', () => {
    const recorder = new EvidenceRecorder('chromium', '1148', testReportPath);
    recorder.addSignalingEvent('gate_01_connection', 'outbound', 'SignalRequest', 'Offer request', {
      access_token: 'secret-token'
    });

    const saved = recorder.saveReport(testReportPath);
    expect(fs.existsSync(testReportPath)).toBe(true);

    const fileContent = JSON.parse(fs.readFileSync(testReportPath, 'utf-8'));
    expect(fileContent.sdkVersion).toBe('2.23.0');
    expect(fileContent.gates.gate_01_connection.signalingEvents[0].rawPayloadRedacted).toContain(
      '[REDACTED]'
    );
  });
});
