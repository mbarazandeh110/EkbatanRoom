import { test, expect, sharedEvidenceRecorder } from './fixtures/test_context.js';
import { gateRegistry } from '../src/controllers/gate_registry.js';
import type { GateCheckResult } from '../src/domain/types.js';
import type { ParticipantClientController } from '../src/controllers/participant_client.js';

test.describe.configure({ mode: 'serial' });

test.describe('Gate 5: ICE Restart & Media Resumption (05-ice-restart)', () => {
  test('trigger real ICE restart and verify signaling renegotiation and positive media resumption', async ({
    createParticipant,
    tokenFactory,
    suiteConfig,
    requireGatePrerequisite
  }) => {
    const gateId = 'gate_05_ice_restart';

    // Enforce sequential gating: Gate 5 requires Gate 2 to have passed
    requireGatePrerequisite(gateId, 'gate_02_bidirectional_rtp');

    sharedEvidenceRecorder.startGate(gateId);

    const roomId = `gate5-ice-restart-room-${Date.now()}`;
    const clientAId = 'peer-alice-c5';
    const clientBId = 'peer-bob-c5';
    const checks: Record<string, GateCheckResult> = {};

    let clientA: ParticipantClientController | undefined;
    let clientB: ParticipantClientController | undefined;

    try {
      // 1. Create Alice and Bob
      clientA = await createParticipant(clientAId, roomId, gateId);
      clientB = await createParticipant(clientBId, roomId, gateId);

      const tokenA = await tokenFactory.createToken(roomId, clientAId, { name: 'Alice ICE Restart' });
      const tokenB = await tokenFactory.createToken(roomId, clientBId, { name: 'Bob Receiver' });

      await clientA.join(suiteConfig.signalingUrl, tokenA);
      await clientA.publishTracks({ audio: true, video: true });

      await clientB.join(suiteConfig.signalingUrl, tokenB);
      await clientB.waitForRemoteParticipant(clientAId, 10000);
      await clientB.waitForRemoteTrack('video', 15000);

      // Verify initial baseline video rendering
      const initialVideoB = await clientB.getRenderedVideoStats('#remote-video');
      expect(initialVideoB.videoWidth).toBeGreaterThan(0);
      expect(initialVideoB.videoHeight).toBeGreaterThan(0);

      // 2. Pre-restart baseline media stats
      const statsPreA = await clientA.getStats();
      const statsPreB = await clientB.getStats();
      const videoPreB = await clientB.getRenderedVideoStats('#remote-video');

      expect(statsPreA.rawCounters.totalBytesSent).toBeGreaterThan(0);
      expect(statsPreB.rawCounters.totalBytesReceived).toBeGreaterThan(0);

      // 3. Trigger real ICE restart on Alice
      await clientA.restartIce();

      // Give signaling and ICE renegotiation time to exchange offer/answer and re-establish connection
      await clientA.getPage().waitForTimeout(2000);

      // Verify connection state remains CONNECTED
      const stateA = await clientA.getPage().evaluate(() => window.testHarness.getState());
      expect(stateA.status).toBe('CONNECTED');

      checks.ice_signaling_transition = {
        name: 'ice_signaling_transition',
        status: 'PASSED',
        description: 'ICE restart triggered and renegotiation completed with stable connection state',
        details: {
          participantStatus: stateA.status,
          connectionState: stateA.connectionState
        }
      };

      // 4. Critical Verification: Media Resumption after ICE restart
      // (Do not treat signaling success alone as proof of recovery; verify post-restart byte delta & rendering)
      await clientB.getPage().waitForTimeout(2000);

      const statsPostA = await clientA.getStats();
      const statsPostB = await clientB.getStats();
      const videoPostB = await clientB.getRenderedVideoStats('#remote-video');

      const bytesSentDeltaA = statsPostA.rawCounters.totalBytesSent - statsPreA.rawCounters.totalBytesSent;
      const bytesReceivedDeltaB = statsPostB.rawCounters.totalBytesReceived - statsPreB.rawCounters.totalBytesReceived;

      sharedEvidenceRecorder.addRtpMetricsSnapshot(gateId, `${clientAId}-post-restart`, statsPostA);
      sharedEvidenceRecorder.addRtpMetricsSnapshot(gateId, `${clientBId}-post-restart`, statsPostB);

      expect(bytesSentDeltaA).toBeGreaterThan(0);
      expect(bytesReceivedDeltaB).toBeGreaterThan(0);
      expect(videoPostB.videoWidth).toBeGreaterThan(0);
      expect(videoPostB.currentTime).toBeGreaterThanOrEqual(videoPreB.currentTime);

      checks.media_resumption_after_restart = {
        name: 'media_resumption_after_restart',
        status: 'PASSED',
        description: `Verified post-restart media resumption: sent delta ${bytesSentDeltaA} bytes, received delta ${bytesReceivedDeltaB} bytes, video progressed at ${videoPostB.videoWidth}x${videoPostB.videoHeight}`,
        details: {
          bytesSentDelta: bytesSentDeltaA,
          bytesReceivedDelta: bytesReceivedDeltaB,
          totalSent: statsPostA.rawCounters.totalBytesSent,
          totalReceived: statsPostB.rawCounters.totalBytesReceived,
          videoWidth: videoPostB.videoWidth,
          videoHeight: videoPostB.videoHeight,
          preTime: videoPreB.currentTime,
          postTime: videoPostB.currentTime
        }
      };

      gateRegistry.setStatus(gateId, 'PASSED');
      sharedEvidenceRecorder.recordGateResult(gateId, 'PASSED', checks);
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      gateRegistry.setStatus(gateId, 'FAILED', errorMsg);
      sharedEvidenceRecorder.recordGateResult(gateId, 'FAILED', checks, errorMsg);
      sharedEvidenceRecorder.markDownstreamBlocked(gateId, `Prerequisite Gate 5 failed: ${errorMsg}`);
      throw err;
    } finally {
      if (clientA) await clientA.leave().catch(() => {});
      if (clientB) await clientB.leave().catch(() => {});
    }
  });
});
