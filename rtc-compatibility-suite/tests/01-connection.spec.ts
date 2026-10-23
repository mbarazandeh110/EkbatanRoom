import { test, expect, sharedEvidenceRecorder } from './fixtures/test_context.js';
import { gateRegistry } from '../src/controllers/gate_registry.js';
import type { GateCheckResult } from '../src/domain/types.js';

test.describe.configure({ mode: 'serial' });

test.describe('Gate 1: Connection & Protocol Handshake (01-connection)', () => {
  test('official livekit-client connects to signaling-coordinator /rtc and completes join', async ({
    createParticipant,
    tokenFactory,
    suiteConfig
  }) => {
    const gateId = 'gate_01_connection';
    sharedEvidenceRecorder.startGate(gateId);

    const roomId = 'gate1-verification-room';
    const identity = 'verifier-gate1';
    const checks: Record<string, GateCheckResult> = {};

    let participant;
    try {
      participant = await createParticipant(identity, roomId, gateId);
      participant.getPage().on('console', msg => console.log('[GATE1 LOG]', msg.type(), msg.text()));
      participant.getPage().on('pageerror', err => console.log('[GATE1 ERROR]', err.message));

      // 1. Create valid test token
      const token = await tokenFactory.createToken(roomId, identity, {
        name: 'Gate 1 Verifier'
      });
      expect(token).toBeDefined();

      // 2. Perform connection and Join handshake via /rtc
      await participant.join(suiteConfig.signalingUrl, token);

      // 3. Verify joined state
      const state = await participant.getPage().evaluate(() => {
        return window.testHarness.getState();
      });

      expect(state.status).toBe('CONNECTED');
      expect(state.connectionState).toBe('connected');
      expect(state.participantIdentity).toBe(identity);

      checks.ws_handshake = {
        name: 'ws_handshake',
        status: 'PASSED',
        description: 'WebSocket connection established to /rtc endpoint'
      };

      checks.join_protocol = {
        name: 'join_protocol',
        status: 'PASSED',
        description: 'Completed protocol v12 JoinResponse exchange'
      };

      checks.participant_state = {
        name: 'participant_state',
        status: 'PASSED',
        description: `Participant enrolled in room ${roomId} with identity ${identity}`
      };

      gateRegistry.setStatus(gateId, 'PASSED');
      sharedEvidenceRecorder.recordGateResult(gateId, 'PASSED', checks);
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      gateRegistry.setStatus(gateId, 'FAILED', errorMsg);
      sharedEvidenceRecorder.recordGateResult(gateId, 'FAILED', checks, errorMsg);
      sharedEvidenceRecorder.markDownstreamBlocked(gateId, `Prerequisite Gate 1 failed: ${errorMsg}`);
      throw err;
    } finally {
      if (participant) {
        await participant.leave().catch(() => {});
      }
    }
  });
});
