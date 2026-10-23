import type { GateId, GateStatus, GateCheckResult } from './types.js';
import type { RtpMetricsSnapshot } from './qoe_metrics.js';

export interface SignalingEvent {
  timestamp: string;
  direction: 'inbound' | 'outbound';
  type: string;
  payloadSummary: string;
  rawPayloadRedacted?: string;
}

export interface GateEvidence {
  gateId: GateId;
  name: string;
  status: GateStatus;
  startTime: string;
  endTime: string;
  durationMs: number;
  checks: Record<string, GateCheckResult>;
  failureReason?: string;
  blockedReason?: string;
  signalingEvents: SignalingEvent[];
  rtpMetrics?: Record<string, RtpMetricsSnapshot>;
  browserLogs?: string[];
  diagnosticDetails?: Record<string, unknown>;
}

export interface EvidenceReport {
  suiteVersion: string;
  sdkVersion: string;
  runId: string;
  timestamp: string;
  browser: {
    name: string;
    version: string;
  };
  overallStatus: 'PASSED' | 'FAILED' | 'PARTIAL' | 'NOT_RUN';
  summary: {
    passed: number;
    failed: number;
    blocked: number;
    notRun: number;
    total: number;
  };
  gates: Record<GateId, GateEvidence>;
}
