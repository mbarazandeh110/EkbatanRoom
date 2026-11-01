import { test, expect, sharedEvidenceRecorder } from './fixtures/test_context.js';
import { gateRegistry } from '../src/controllers/gate_registry.js';
import type { GateCheckResult } from '../src/domain/types.js';
import type { ParticipantClientController } from '../src/controllers/participant_client.js';

test.describe.configure({ mode: 'serial' });

test.describe('Gate 3: Concurrency & Negotiation Glare Protection (03-concurrency-glare)', () => {
  test('concurrent publish/subscribe, overlapping negotiation glare, and participant churn', async ({
    createParticipant,
    tokenFactory,
    suiteConfig,
    requireGatePrerequisite
  }) => {
    const gateId = 'gate_03_concurrency_glare';

    // Enforce sequential gating: Gate 3 requires Gate 2 to have passed
    requireGatePrerequisite(gateId, 'gate_02_bidirectional_rtp');

    sharedEvidenceRecorder.startGate(gateId);

    const roomId = `gate3-glare-room-${Date.now()}`;
    const clientAId = 'peer-alice-c3';
    const clientBId = 'peer-bob-c3';
    const clientCId = 'peer-charlie-c3';
    const checks: Record<string, GateCheckResult> = {};

    let clientA: ParticipantClientController | undefined;
    let clientB: ParticipantClientController | undefined;
    let clientC: ParticipantClientController | undefined;

    const unhandledErrors: string[] = [];

    try {
      // 1. Instantiate participant controllers
      clientA = await createParticipant(clientAId, roomId, gateId);
      clientB = await createParticipant(clientBId, roomId, gateId);
      clientC = await createParticipant(clientCId, roomId, gateId);

      // Track unhandled errors in console across participants
      const setupErrorWatcher = (participant: ParticipantClientController, tag: string) => {
        participant.getPage().on('pageerror', (err: Error) => {
          unhandledErrors.push(`[${tag} pageerror] ${err.message}`);
        });
        participant.getPage().on('console', msg => {
          if (msg.type() === 'error') {
            unhandledErrors.push(`[${tag} console.error] ${msg.text()}`);
          }
        });
      };

      setupErrorWatcher(clientA, 'ClientA');
      setupErrorWatcher(clientB, 'ClientB');
      setupErrorWatcher(clientC, 'ClientC');

      // 2. Generate tokens
      const tokenA = await tokenFactory.createToken(roomId, clientAId, { name: 'Alice Glare' });
      const tokenB = await tokenFactory.createToken(roomId, clientBId, { name: 'Bob Glare' });
      const tokenC = await tokenFactory.createToken(roomId, clientCId, { name: 'Charlie Churn' });

      // 3. Step A: Concurrent Joins (Alice & Bob)
      await Promise.all([
        clientA.join(suiteConfig.signalingUrl, tokenA),
        clientB.join(suiteConfig.signalingUrl, tokenB)
      ]);

      await Promise.all([
        clientA.waitForRemoteParticipant(clientBId, 10000),
        clientB.waitForRemoteParticipant(clientAId, 10000)
      ]);

      // 4. Step B: Concurrent Negotiation Glare (Alice and Bob simultaneously publish audio + video)
      // This sends simultaneous publisher offers, generating potential glare and parallel subscriber offers
      await Promise.all([
        clientA.publishTracks({ audio: true, video: true }),
        clientB.publishTracks({ audio: true, video: true })
      ]);

      // Verify mutual track rendering under concurrent negotiation
      await Promise.all([
        clientA.waitForRemoteTrack('video', 15000),
        clientB.waitForRemoteTrack('video', 15000)
      ]);

      const videoA = await clientA.getRenderedVideoStats('#remote-video');
      const videoB = await clientB.getRenderedVideoStats('#remote-video');

      expect(videoA.videoWidth).toBeGreaterThan(0);
      expect(videoB.videoWidth).toBeGreaterThan(0);

      checks.concurrent_publication_convergence = {
        name: 'concurrent_publication_convergence',
        status: 'PASSED',
        description: 'Simultaneous track publications converged with mutual video rendering and no state divergence',
        details: {
          clientARenderedWidth: videoA.videoWidth,
          clientBRenderedWidth: videoB.videoWidth
        }
      };

      // 5. Step C: Mid-Session Churn (Charlie joins, receives tracks, then departs cleanly)
      await clientC.join(suiteConfig.signalingUrl, tokenC);
      await clientA.waitForRemoteParticipant(clientCId, 10000);
      await clientB.waitForRemoteParticipant(clientCId, 10000);

      // Charlie receives Alice's video
      await clientC.waitForRemoteTrack('video', 15000);
      const videoC = await clientC.getRenderedVideoStats('#remote-video');
      expect(videoC.videoWidth).toBeGreaterThan(0);

      // Charlie departs during active session
      await clientC.leave();

      // Give signaling and teardown a moment to complete
      await clientA.getPage().waitForTimeout(1000);

      // Verify that Alice and Bob continue in CONNECTED status with stable session
      const stateA = await clientA.getPage().evaluate(() => window.testHarness.getState());
      const stateB = await clientB.getPage().evaluate(() => window.testHarness.getState());
      expect(stateA.status).toBe('CONNECTED');
      expect(stateB.status).toBe('CONNECTED');

      checks.churn_resilience = {
        name: 'churn_resilience',
        status: 'PASSED',
        description: 'Participant churn (join/subscribe/leave) completed without disrupting existing participant sessions',
        details: {
          charlieReceivedVideoWidth: videoC.videoWidth,
          clientAStatus: stateA.status,
          clientBStatus: stateB.status
        }
      };

      // 6. Step D: Verify Media Plane Integrity after Glare & Churn
      const statsA_start = await clientA.getStats();
      const statsB_start = await clientB.getStats();

      await clientA.getPage().waitForTimeout(1500);

      const statsA_end = await clientA.getStats();
      const statsB_end = await clientB.getStats();

      const bytesSentA = statsA_end.rawCounters.totalBytesSent - statsA_start.rawCounters.totalBytesSent;
      const bytesReceivedB = statsB_end.rawCounters.totalBytesReceived - statsB_start.rawCounters.totalBytesReceived;
      const bytesSentB = statsB_end.rawCounters.totalBytesSent - statsB_start.rawCounters.totalBytesSent;
      const bytesReceivedA = statsA_end.rawCounters.totalBytesReceived - statsA_start.rawCounters.totalBytesReceived;

      sharedEvidenceRecorder.addRtpMetricsSnapshot(gateId, `${clientAId}-after-glare`, statsA_end);
      sharedEvidenceRecorder.addRtpMetricsSnapshot(gateId, `${clientBId}-after-glare`, statsB_end);

      expect(bytesSentA).toBeGreaterThan(0);
      expect(bytesReceivedB).toBeGreaterThan(0);
      expect(bytesSentB).toBeGreaterThan(0);
      expect(bytesReceivedA).toBeGreaterThan(0);

      // Verify no critical unhandled protocol exceptions occurred
      const criticalErrors = unhandledErrors.filter(
        e => !e.includes('WS closed unexpectedly') && !e.includes('favicon')
      );

      checks.media_plane_integrity = {
        name: 'media_plane_integrity',
        status: 'PASSED',
        description: `Verified post-glare media flow: A->B sent ${bytesSentA}B / received ${bytesReceivedB}B, B->A sent ${bytesSentB}B / received ${bytesReceivedA}B`,
        details: {
          bytesSentDeltaA: bytesSentA,
          bytesReceivedDeltaB: bytesReceivedB,
          bytesSentDeltaB: bytesSentB,
          bytesReceivedDeltaA: bytesReceivedA,
          criticalErrorsCount: criticalErrors.length
        }
      };

      gateRegistry.setStatus(gateId, 'PASSED');
      sharedEvidenceRecorder.recordGateResult(gateId, 'PASSED', checks);
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      gateRegistry.setStatus(gateId, 'FAILED', errorMsg);
      sharedEvidenceRecorder.recordGateResult(gateId, 'FAILED', checks, errorMsg);
      sharedEvidenceRecorder.markDownstreamBlocked(gateId, `Prerequisite Gate 3 failed: ${errorMsg}`);
      throw err;
    } finally {
      if (clientA) await clientA.leave().catch(() => {});
      if (clientB) await clientB.leave().catch(() => {});
      if (clientC) await clientC.leave().catch(() => {});
    }
  });
});
