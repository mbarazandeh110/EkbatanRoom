import * as fs from 'fs';
import * as path from 'path';
import type { GateId, GateStatus } from '../domain/types.js';

export class GateExecutionRegistry {
  private static instance: GateExecutionRegistry;
  private gateStatuses: Map<GateId, GateStatus> = new Map();
  private failureReasons: Map<GateId, string> = new Map();
  private stateFilePath?: string;

  constructor(stateFilePath?: string) {
    this.stateFilePath = stateFilePath;
    this.loadFromDisk();
  }

  private loadFromDisk(): void {
    if (this.stateFilePath && fs.existsSync(this.stateFilePath)) {
      try {
        const content = JSON.parse(fs.readFileSync(this.stateFilePath, 'utf-8'));
        if (content.statuses) {
          for (const [k, v] of Object.entries(content.statuses)) {
            // Keep existing entry if not present or if disk has PASSED
            if (!this.gateStatuses.has(k as GateId) || v === 'PASSED') {
              this.gateStatuses.set(k as GateId, v as GateStatus);
            }
          }
        }
        if (content.reasons) {
          for (const [k, v] of Object.entries(content.reasons)) {
            if (!this.failureReasons.has(k as GateId)) {
              this.failureReasons.set(k as GateId, v as string);
            }
          }
        }
      } catch {}
    }
  }

  private saveToDisk(): void {
    if (!this.stateFilePath) return;
    try {
      const dir = path.dirname(this.stateFilePath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      const data = {
        statuses: Object.fromEntries(this.gateStatuses),
        reasons: Object.fromEntries(this.failureReasons)
      };
      fs.writeFileSync(this.stateFilePath, JSON.stringify(data, null, 2), 'utf-8');
    } catch {}
  }

  public static getInstance(stateFilePath?: string): GateExecutionRegistry {
    if (!GateExecutionRegistry.instance) {
      GateExecutionRegistry.instance = new GateExecutionRegistry(
        stateFilePath || path.resolve(process.cwd(), 'test-results/gate-registry.json')
      );
    }
    return GateExecutionRegistry.instance;
  }

  public setStatus(gateId: GateId, status: GateStatus, reason?: string): void {
    this.loadFromDisk();
    this.gateStatuses.set(gateId, status);
    if (reason) {
      this.failureReasons.set(gateId, reason);
    }
    this.saveToDisk();
  }

  public getStatus(gateId: GateId): GateStatus {
    this.loadFromDisk();
    return this.gateStatuses.get(gateId) || 'NOT_RUN';
  }

  public getFailureReason(gateId: GateId): string | undefined {
    this.loadFromDisk();
    return this.failureReasons.get(gateId);
  }

  public isPrerequisiteMet(prerequisiteGateId: GateId): boolean {
    return this.getStatus(prerequisiteGateId) === 'PASSED';
  }

  public reset(): void {
    this.gateStatuses.clear();
    this.failureReasons.clear();
    try {
      if (this.stateFilePath && fs.existsSync(this.stateFilePath)) {
        fs.unlinkSync(this.stateFilePath);
      }
    } catch {}
  }
}

export const gateRegistry = GateExecutionRegistry.getInstance();

