/**
 * Formal Gate Statuses as specified in the test suite requirements.
 */
export type GateStatus = 'PASSED' | 'FAILED' | 'BLOCKED' | 'NOT_RUN';

export type GateId =
  | 'gate_01_connection'
  | 'gate_02_bidirectional_rtp'
  | 'gate_03_concurrency_glare'
  | 'gate_04_simulcast_layers'
  | 'gate_05_ice_restart'
  | 'gate_06_datachannels_twirp_moderation'
  | 'gate_07_multinode_cascading_and_licensing';

export interface GateCheckResult {
  name: string;
  status: GateStatus;
  description: string;
  details?: Record<string, unknown>;
  error?: string;
}

export interface GateExecutionResult {
  gateId: GateId;
  name: string;
  status: GateStatus;
  startTime: string;
  endTime: string;
  durationMs: number;
  checks: Record<string, GateCheckResult>;
  failureReason?: string;
  blockedReason?: string;
}
