import { spawn, execSync, type ChildProcess } from 'child_process';
import * as path from 'path';

export interface GeneratedLicense {
  publicKeyHex: string;
  privateKeyHex: string;
  token: string;
}

export interface LicenseOptions {
  maxPeers?: number;
  maxRooms?: number;
  tier?: 'enterprise' | 'community';
  customer?: string;
  days?: number;
}

export interface LicensedCoordinatorProcess {
  port: number;
  signalingUrl: string;
  stop: () => Promise<void>;
  getLogs: () => string[];
}

export class LicenseTestManager {
  private coordinatorBinPath: string;
  private licenseGenPath: string;

  constructor() {
    this.coordinatorBinPath = path.resolve(process.cwd(), '../signaling-coordinator/bin/signaling-coordinator');
    this.licenseGenPath = path.resolve(process.cwd(), '../signaling-coordinator/tools/license-gen/main.go');
  }

  public generateLicense(options: LicenseOptions = {}): GeneratedLicense {
    const keyOut = execSync(`go run ${this.licenseGenPath} -gen-keys`).toString();
    const pubMatch = keyOut.match(/PUBLIC_KEY_HEX:\s+([a-f0-9]+)/i);
    const privMatch = keyOut.match(/PRIVATE_KEY_HEX:\s+([a-f0-9]+)/i);

    if (!pubMatch || !privMatch) {
      throw new Error(`Failed to generate Ed25519 keypair: ${keyOut}`);
    }

    const publicKeyHex = pubMatch[1];
    const privateKeyHex = privMatch[1];

    const maxPeers = options.maxPeers ?? 5000;
    const maxRooms = options.maxRooms ?? 200;
    const tier = options.tier ?? 'enterprise';
    const customer = options.customer ?? 'enterprise-client';
    const days = options.days ?? 365;

    const tokenOut = execSync(
      `go run ${this.licenseGenPath} -priv-key ${privateKeyHex} -max-peers ${maxPeers} -max-rooms ${maxRooms} -tier ${tier} -customer ${customer} -days ${days}`
    ).toString();

    const tokenMatch = tokenOut.match(/TOKEN:\s*\n([^\s]+)/);
    if (!tokenMatch) {
      throw new Error(`Failed to parse license token from generator: ${tokenOut}`);
    }

    return {
      publicKeyHex,
      privateKeyHex,
      token: tokenMatch[1]
    };
  }

  public async startLicensedCoordinator(
    port: number,
    license: GeneratedLicense,
    sfuGrpcAddr = '127.0.0.1:50051,127.0.0.1:50052'
  ): Promise<LicensedCoordinatorProcess> {
    const logs: string[] = [];

    const proc: ChildProcess = spawn(this.coordinatorBinPath, [], {
      env: {
        ...process.env,
        PORT: String(port),
        SFU_NODES: sfuGrpcAddr,
        SFU_GRPC_ADDR: sfuGrpcAddr,
        RTC_ICE_SERVERS: '',
        EKBATAN_LICENSE_PUBLIC_KEY: license.publicKeyHex,
        EKBATAN_LICENSE_KEY: license.token,
        OBS_LOG_LEVEL: 'debug',
        OBS_LOG_FORMAT: 'json'
      }
    });

    proc.stdout?.on('data', (data) => {
      const line = data.toString().trim();
      logs.push(line);
    });

    proc.stderr?.on('data', (data) => {
      const line = data.toString().trim();
      logs.push(line);
    });

    // Wait until coordinator starts listening or fails
    const startTime = Date.now();
    let isListening = false;

    while (Date.now() - startTime < 8000) {
      if (logs.some((l) => l.includes('server listening') || l.includes(`port":"${port}"`))) {
        isListening = true;
        break;
      }
      if (proc.exitCode !== null) {
        throw new Error(
          `Licensed coordinator exited prematurely with code ${proc.exitCode}. Logs:\n${logs.join('\n')}`
        );
      }
      await new Promise((r) => setTimeout(r, 100));
    }

    if (!isListening) {
      proc.kill('SIGKILL');
      throw new Error(`Timeout waiting for licensed coordinator to listen on port ${port}. Logs:\n${logs.join('\n')}`);
    }

    const signalingUrl = `ws://127.0.0.1:${port}`;

    return {
      port,
      signalingUrl,
      getLogs: () => [...logs],
      stop: async () => {
        if (proc.exitCode === null) {
          proc.kill('SIGTERM');
          await new Promise<void>((resolve) => {
            const timeout = setTimeout(() => {
              if (proc.exitCode === null) {
                proc.kill('SIGKILL');
              }
              resolve();
            }, 3000);
            proc.on('exit', () => {
              clearTimeout(timeout);
              resolve();
            });
          });
        }
      }
    };
  }
}
