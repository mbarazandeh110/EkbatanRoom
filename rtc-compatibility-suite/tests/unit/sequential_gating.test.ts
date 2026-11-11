import { describe, it, expect, beforeEach } from 'vitest';
import { GateExecutionRegistry } from '../../src/controllers/gate_registry.js';
import { EvidenceRecorder } from '../../src/reporters/evidence_recorder.js';

describe('Sequential Gate Dependencies & Failure Propagation', () => {
  let registry: GateExecutionRegistry;
  let recorder: EvidenceRecorder;

  beforeEach(() => {
    registry = new GateExecutionRegistry();
    recorder = new EvidenceRecorder('chromium', '1148');
  });

  it('blocks Gate 2 when Gate 1 fails or has not passed', () => {
    // Gate 1 has not run
    expect(registry.isPrerequisiteMet('gate_01_connection')).toBe(false);

    // Simulate Gate 1 failing
    const failureMsg = 'WebSocket handshake failed with 502 Bad Gateway';
    registry.setStatus('gate_01_connection', 'FAILED', failureMsg);
    recorder.startGate('gate_01_connection');
    recorder.recordGateResult('gate_01_connection', 'FAILED', {}, failureMsg);

    expect(registry.isPrerequisiteMet('gate_01_connection')).toBe(false);

    // Downstream gates blocked
    recorder.markDownstreamBlocked('gate_01_connection', `Prerequisite gate_01_connection failed: ${failureMsg}`);

    const report = recorder.generateReport();
    expect(report.overallStatus).toBe('FAILED');
    expect(report.gates.gate_01_connection.status).toBe('FAILED');
    expect(report.gates.gate_01_connection.failureReason).toBe(failureMsg);

    // Gates 2 through 7 must be BLOCKED, never PASSED
    expect(report.gates.gate_02_bidirectional_rtp.status).toBe('BLOCKED');
    expect(report.gates.gate_02_bidirectional_rtp.blockedReason).toContain(failureMsg);
    expect(report.gates.gate_03_concurrency_glare.status).toBe('BLOCKED');
    expect(report.gates.gate_04_simulcast_layers.status).toBe('BLOCKED');
    expect(report.gates.gate_05_ice_restart.status).toBe('BLOCKED');
    expect(report.gates.gate_06_datachannels_twirp_moderation.status).toBe('BLOCKED');
    expect(report.gates.gate_07_multinode_cascading_and_licensing.status).toBe('BLOCKED');

    expect(report.summary.passed).toBe(0);
    expect(report.summary.failed).toBe(1);
    expect(report.summary.blocked).toBe(6);
    expect(report.summary.notRun).toBe(0);
  });

  it('blocks Gates 3-7 when Gate 2 fails after Gate 1 passed', () => {
    // Gate 1 passed
    registry.setStatus('gate_01_connection', 'PASSED');
    recorder.startGate('gate_01_connection');
    recorder.recordGateResult('gate_01_connection', 'PASSED', {
      conn: { name: 'conn', status: 'PASSED', description: 'connected' }
    });

    expect(registry.isPrerequisiteMet('gate_01_connection')).toBe(true);

    // Gate 2 fails (e.g. no RTP media packets received)
    const mediaFailureMsg = 'No inbound RTP packets received (bytesReceived == 0)';
    registry.setStatus('gate_02_bidirectional_rtp', 'FAILED', mediaFailureMsg);
    recorder.startGate('gate_02_bidirectional_rtp');
    recorder.recordGateResult('gate_02_bidirectional_rtp', 'FAILED', {}, mediaFailureMsg);

    recorder.markDownstreamBlocked('gate_02_bidirectional_rtp', `Prerequisite gate_02_bidirectional_rtp failed: ${mediaFailureMsg}`);

    const report = recorder.generateReport();
    expect(report.overallStatus).toBe('FAILED');
    expect(report.gates.gate_01_connection.status).toBe('PASSED');
    expect(report.gates.gate_02_bidirectional_rtp.status).toBe('FAILED');
    expect(report.gates.gate_03_concurrency_glare.status).toBe('BLOCKED');
    expect(report.gates.gate_04_simulcast_layers.status).toBe('BLOCKED');
    expect(report.gates.gate_05_ice_restart.status).toBe('BLOCKED');
    expect(report.gates.gate_06_datachannels_twirp_moderation.status).toBe('BLOCKED');
    expect(report.gates.gate_07_multinode_cascading_and_licensing.status).toBe('BLOCKED');

    expect(report.summary.passed).toBe(1);
    expect(report.summary.failed).toBe(1);
    expect(report.summary.blocked).toBe(5);
  });
});
