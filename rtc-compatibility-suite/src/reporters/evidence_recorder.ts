import * as fs from 'fs';
import * as path from 'path';
import type { GateId, GateStatus, GateCheckResult } from '../domain/types.js';
import type { EvidenceReport, GateEvidence, SignalingEvent } from '../domain/evidence.js';
import type { RtpMetricsSnapshot } from '../domain/qoe_metrics.js';
import { sanitizePayload } from './redaction.js';

export const PINNED_LIVEKIT_SDK_VERSION = '2.23.0';

const ALL_GATE_IDS: GateId[] = [
  'gate_01_connection',
  'gate_02_bidirectional_rtp',
  'gate_03_concurrency_glare',
  'gate_04_simulcast_layers',
  'gate_05_ice_restart'
];

const GATE_NAMES: Record<GateId, string> = {
  gate_01_connection: 'Gate 1: Connection & Protocol Handshake',
  gate_02_bidirectional_rtp: 'Gate 2: Bidirectional Media RTP & Video Rendering',
  gate_03_concurrency_glare: 'Gate 3: Concurrency & Negotiation Glare Protection',
  gate_04_simulcast_layers: 'Gate 4: Simulcast Layers & Quality Update Mapping',
  gate_05_ice_restart: 'Gate 5: ICE Restart & Media Resumption'
};

export class EvidenceRecorder {
  private runId: string;
  private startTime: string;
  private browserInfo: { name: string; version: string };
  private gates: Record<GateId, GateEvidence>;
  private reportPath: string;

  constructor(
    browserName = 'chromium',
    browserVersion = 'headful/headless-1148',
    reportPath = 'test-results/evidence-report.json'
  ) {
    this.runId = `run-${Date.now()}`;
    this.startTime = new Date().toISOString();
    this.browserInfo = { name: browserName, version: browserVersion };
    this.reportPath = reportPath;

    this.gates = {} as Record<GateId, GateEvidence>;
    for (const gid of ALL_GATE_IDS) {
      this.gates[gid] = {
        gateId: gid,
        name: GATE_NAMES[gid],
        status: 'NOT_RUN',
        startTime: '',
        endTime: '',
        durationMs: 0,
        checks: {},
        signalingEvents: []
      };
    }
  }

  public setBrowserInfo(name: string, version: string): void {
    this.browserInfo = { name, version };
  }

  public startGate(gateId: GateId): void {
    const gate = this.gates[gateId];
    if (gate) {
      gate.startTime = new Date().toISOString();
    }
  }

  public recordGateResult(
    gateId: GateId,
    status: GateStatus,
    checks: Record<string, GateCheckResult>,
    failureReason?: string,
    blockedReason?: string,
    diagnosticDetails?: Record<string, unknown>
  ): void {
    const gate = this.gates[gateId];
    if (!gate) return;

    const endTime = new Date().toISOString();
    const startMs = gate.startTime ? new Date(gate.startTime).getTime() : Date.now();
    const endMs = new Date(endTime).getTime();

    gate.status = status;
    gate.endTime = endTime;
    gate.durationMs = Math.max(0, endMs - startMs);
    gate.checks = checks;
    gate.failureReason = failureReason;
    gate.blockedReason = blockedReason;
    gate.diagnosticDetails = diagnosticDetails;
  }

  public markDownstreamBlocked(failedGateId: GateId, reason: string): void {
    const failedIndex = ALL_GATE_IDS.indexOf(failedGateId);
    if (failedIndex === -1) return;

    for (let i = failedIndex + 1; i < ALL_GATE_IDS.length; i++) {
      const gid = ALL_GATE_IDS[i];
      if (this.gates[gid].status === 'NOT_RUN') {
        this.gates[gid].status = 'BLOCKED';
        this.gates[gid].blockedReason = reason;
      }
    }
  }

  public addSignalingEvent(
    gateId: GateId,
    direction: 'inbound' | 'outbound',
    type: string,
    payloadSummary: string,
    rawPayload?: unknown,
    allowRawSdp = false,
    isDebugLevel = false
  ): void {
    const gate = this.gates[gateId];
    if (!gate) return;

    const event: SignalingEvent = {
      timestamp: new Date().toISOString(),
      direction,
      type,
      payloadSummary: sanitizePayload(payloadSummary, false, false),
      rawPayloadRedacted: rawPayload !== undefined
        ? sanitizePayload(rawPayload, allowRawSdp, isDebugLevel)
        : undefined
    };

    gate.signalingEvents.push(event);
  }

  public addRtpMetricsSnapshot(gateId: GateId, participantKey: string, snapshot: RtpMetricsSnapshot): void {
    const gate = this.gates[gateId];
    if (!gate) return;

    if (!gate.rtpMetrics) {
      gate.rtpMetrics = {};
    }
    gate.rtpMetrics[participantKey] = snapshot;
  }

  public addBrowserLog(gateId: GateId, logLine: string): void {
    const gate = this.gates[gateId];
    if (!gate) return;

    if (!gate.browserLogs) {
      gate.browserLogs = [];
    }
    gate.browserLogs.push(sanitizePayload(logLine, false, false));
  }

  public generateReport(): EvidenceReport {
    let passed = 0;
    let failed = 0;
    let blocked = 0;
    let notRun = 0;

    for (const gid of ALL_GATE_IDS) {
      const status = this.gates[gid].status;
      if (status === 'PASSED') passed++;
      else if (status === 'FAILED') failed++;
      else if (status === 'BLOCKED') blocked++;
      else if (status === 'NOT_RUN') notRun++;
    }

    let overallStatus: EvidenceReport['overallStatus'] = 'NOT_RUN';
    if (failed > 0) {
      overallStatus = 'FAILED';
    } else if (passed === ALL_GATE_IDS.length) {
      overallStatus = 'PASSED';
    } else if (passed > 0) {
      overallStatus = 'PARTIAL';
    }

    return {
      suiteVersion: '1.0.0',
      sdkVersion: PINNED_LIVEKIT_SDK_VERSION,
      runId: this.runId,
      timestamp: this.startTime,
      browser: this.browserInfo,
      overallStatus,
      summary: {
        passed,
        failed,
        blocked,
        notRun,
        total: ALL_GATE_IDS.length
      },
      gates: { ...this.gates }
    };
  }

  public saveReport(targetPath?: string): EvidenceReport {
    const report = this.generateReport();
    const dest = targetPath || this.reportPath;
    const dir = path.dirname(dest);

    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    fs.writeFileSync(dest, JSON.stringify(report, null, 2), 'utf-8');
    return report;
  }
}
