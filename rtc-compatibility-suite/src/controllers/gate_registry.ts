import type { GateId, GateStatus } from '../domain/types.js';

export class GateExecutionRegistry {
  private static instance: GateExecutionRegistry;
  private gateStatuses: Map<GateId, GateStatus> = new Map();
  private failureReasons: Map<GateId, string> = new Map();

  public static getInstance(): GateExecutionRegistry {
    if (!GateExecutionRegistry.instance) {
      GateExecutionRegistry.instance = new GateExecutionRegistry();
    }
    return GateExecutionRegistry.instance;
  }

  public setStatus(gateId: GateId, status: GateStatus, reason?: string): void {
    this.gateStatuses.set(gateId, status);
    if (reason) {
      this.failureReasons.set(gateId, reason);
    }
  }

  public getStatus(gateId: GateId): GateStatus {
    return this.gateStatuses.get(gateId) || 'NOT_RUN';
  }

  public getFailureReason(gateId: GateId): string | undefined {
    return this.failureReasons.get(gateId);
  }

  public isPrerequisiteMet(prerequisiteGateId: GateId): boolean {
    return this.gateStatuses.get(prerequisiteGateId) === 'PASSED';
  }

  public reset(): void {
    this.gateStatuses.clear();
    this.failureReasons.clear();
  }
}

export const gateRegistry = GateExecutionRegistry.getInstance();
