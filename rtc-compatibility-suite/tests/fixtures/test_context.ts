import { test as base, type Page, type BrowserContext } from '@playwright/test';
import { ParticipantClientController } from '../../src/controllers/participant_client.js';
import { TokenFactory } from '../../src/controllers/token_factory.js';
import { EvidenceRecorder } from '../../src/reporters/evidence_recorder.js';
import { gateRegistry } from '../../src/controllers/gate_registry.js';
import { loadSuiteConfig } from '../../src/domain/config.js';
import type { GateId } from '../../src/domain/types.js';
import * as path from 'path';

export interface SuiteFixtures {
  suiteConfig: ReturnType<typeof loadSuiteConfig>;
  evidenceRecorder: EvidenceRecorder;
  tokenFactory: TokenFactory;
  createParticipant: (
    identity: string,
    roomId?: string,
    activeGateId?: GateId
  ) => Promise<ParticipantClientController>;
  requireGatePrerequisite: (gateId: GateId, prerequisiteId: GateId) => void;
}

export const sharedEvidenceRecorder = new EvidenceRecorder();

export const test = base.extend<SuiteFixtures>({
  suiteConfig: async ({}, use) => {
    const config = loadSuiteConfig();
    await use(config);
  },

  evidenceRecorder: [
    async ({ browser }, use) => {
      sharedEvidenceRecorder.setBrowserInfo(browser.browserType().name(), browser.version());
      await use(sharedEvidenceRecorder);
      sharedEvidenceRecorder.saveReport(path.resolve(process.cwd(), 'test-results/evidence-report.json'));
    },
    { auto: true }
  ],

  tokenFactory: async ({}, use) => {
    const factory = new TokenFactory();
    await use(factory);
  },

  requireGatePrerequisite: async ({}, use) => {
    const checkPrereq = (gateId: GateId, prerequisiteId: GateId) => {
      if (process.env.SKIP_GATE_PREREQUISITES !== 'true' && !gateRegistry.isPrerequisiteMet(prerequisiteId)) {
        const prereqStatus = gateRegistry.getStatus(prerequisiteId);
        const reason = `Prerequisite gate ${prerequisiteId} status is '${prereqStatus}' (must be 'PASSED')`;
        gateRegistry.setStatus(gateId, 'BLOCKED', reason);
        sharedEvidenceRecorder.recordGateResult(gateId, 'BLOCKED', {}, undefined, reason);
        sharedEvidenceRecorder.saveReport();
        test.skip(true, reason);
      }
    };
    await use(checkPrereq);
  },

  createParticipant: async ({ browser, baseURL, suiteConfig }, use) => {
    const createdContexts: BrowserContext[] = [];

    const factory = async (identity: string, roomId = 'compatibility-room', activeGateId?: GateId) => {
      const context = await browser.newContext({
        permissions: ['camera', 'microphone'],
        ignoreHTTPSErrors: true
      });
      createdContexts.push(context);

      const page = await context.newPage();
      const client = new ParticipantClientController(page, identity, roomId);

      // Attach WebSocket listener to record signaling events into evidence
      client.attachWebSocketListener((direction, payload) => {
        if (!activeGateId) return;
        const size = typeof payload === 'string' ? payload.length : payload.byteLength;
        sharedEvidenceRecorder.addSignalingEvent(
          activeGateId,
          direction,
          'WebSocketFrame',
          `${direction} frame (${size} bytes)`,
          typeof payload === 'string' ? payload : `[Binary protobuf frame: ${size} bytes]`,
          suiteConfig.observability.logRawSdp,
          suiteConfig.observability.logLevel === 'debug' || suiteConfig.observability.logLevel === 'trace'
        );
      });

      // Attach browser console listener if enabled
      if (suiteConfig.observability.captureBrowserLogs && activeGateId) {
        client.attachConsoleListener((line) => {
          sharedEvidenceRecorder.addBrowserLog(activeGateId, line);
        });
      }

      await client.navigate(baseURL || 'http://localhost:5173');
      return client;
    };

    await use(factory);

    // Teardown created contexts
    for (const ctx of createdContexts) {
      await ctx.close().catch(() => {});
    }
  }
});

export { expect } from '@playwright/test';
