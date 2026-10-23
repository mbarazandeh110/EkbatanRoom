import { spawn, ChildProcess } from 'child_process';
import * as http from 'http';
import * as net from 'net';
import * as path from 'path';

export class ServiceProcessManager {
  private coordinatorProcess: ChildProcess | null = null;
  private sfuProcess: ChildProcess | null = null;

  public static async isPortOpen(port: number, host = '127.0.0.1', timeoutMs = 1000): Promise<boolean> {
    return new Promise((resolve) => {
      const socket = new net.Socket();
      socket.setTimeout(timeoutMs);

      socket.once('connect', () => {
        socket.destroy();
        resolve(true);
      });

      socket.once('timeout', () => {
        socket.destroy();
        resolve(false);
      });

      socket.once('error', () => {
        resolve(false);
      });

      socket.connect(port, host);
    });
  }

  public static async checkHttpHealth(url = 'http://127.0.0.1:7880/healthz', timeoutMs = 1000): Promise<boolean> {
    return new Promise((resolve) => {
      const req = http.get(url, { timeout: timeoutMs }, (res) => {
        resolve(res.statusCode === 200);
      });
      req.on('error', () => resolve(false));
      req.on('timeout', () => {
        req.destroy();
        resolve(false);
      });
    });
  }

  public async ensureServicesRunning(): Promise<void> {
    const sfuRunning = await ServiceProcessManager.isPortOpen(50051);
    if (!sfuRunning) {
      const sfuBin = path.resolve(process.cwd(), '../sfu-media-node/target/debug/sfu-media-node');
      this.sfuProcess = spawn(sfuBin, [], {
        env: {
          ...process.env,
          OBS_LOG_LEVEL: process.env.OBS_LOG_LEVEL || 'info',
          OBS_LOG_FORMAT: 'json'
        },
        stdio: 'ignore'
      });
      await this.waitForCondition(async () => ServiceProcessManager.isPortOpen(50051), 10000, 'SFU Media Node');
    }

    const coordinatorRunning = await ServiceProcessManager.checkHttpHealth();
    if (!coordinatorRunning) {
      const coordBin = path.resolve(process.cwd(), '../signaling-coordinator/bin/signaling-coordinator');
      this.coordinatorProcess = spawn(coordBin, [], {
        env: {
          ...process.env,
          PORT: '7880',
          SFU_GRPC_ADDR: '127.0.0.1:50051',
          OBS_LOG_LEVEL: process.env.OBS_LOG_LEVEL || 'info',
          OBS_LOG_FORMAT: 'json'
        },
        stdio: 'ignore'
      });
      await this.waitForCondition(async () => ServiceProcessManager.checkHttpHealth(), 10000, 'Signaling Coordinator');
    }
  }

  private async waitForCondition(
    condition: () => Promise<boolean>,
    timeoutMs: number,
    serviceName: string
  ): Promise<void> {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      if (await condition()) {
        return;
      }
      await new Promise((r) => setTimeout(r, 250));
    }
    throw new Error(`Timeout waiting for ${serviceName} to become ready after ${timeoutMs}ms`);
  }

  public stopSpawnedServices(): void {
    if (this.coordinatorProcess && !this.coordinatorProcess.killed) {
      this.coordinatorProcess.kill('SIGTERM');
      this.coordinatorProcess = null;
    }
    if (this.sfuProcess && !this.sfuProcess.killed) {
      this.sfuProcess.kill('SIGTERM');
      this.sfuProcess = null;
    }
  }
}
