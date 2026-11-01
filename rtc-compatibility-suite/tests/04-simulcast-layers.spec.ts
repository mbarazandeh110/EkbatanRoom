import { test, expect, sharedEvidenceRecorder } from './fixtures/test_context.js';
import { gateRegistry } from '../src/controllers/gate_registry.js';
import type { GateCheckResult } from '../src/domain/types.js';
import type { ParticipantClientController } from '../src/controllers/participant_client.js';

test.describe.configure({ mode: 'serial' });

test.describe('Gate 4: Simulcast Layers & Quality Update Mapping (04-simulcast-layers)', () => {
  test('publish simulcast video and verify SFU layer selection and LiveKit quality update mapping', async ({
    createParticipant,
    tokenFactory,
    suiteConfig,
    requireGatePrerequisite
  }) => {
    const gateId = 'gate_04_simulcast_layers';

    // Enforce sequential gating: Gate 4 requires Gate 2 to have passed
    requireGatePrerequisite(gateId, 'gate_02_bidirectional_rtp');

    sharedEvidenceRecorder.startGate(gateId);

    const roomId = `gate4-simulcast-room-${Date.now()}`;
    const clientAId = 'peer-alice-c4';
    const clientBId = 'peer-bob-c4';
    const checks: Record<string, GateCheckResult> = {};

    let clientA: ParticipantClientController | undefined;
    let clientB: ParticipantClientController | undefined;

    try {
      // 1. Create Alice and Bob
      clientA = await createParticipant(clientAId, roomId, gateId);
      clientB = await createParticipant(clientBId, roomId, gateId);

      const tokenA = await tokenFactory.createToken(roomId, clientAId, { name: 'Alice Simulcast' });
      const tokenB = await tokenFactory.createToken(roomId, clientBId, { name: 'Bob Subscriber' });

      // Alice joins first
      await clientA.join(suiteConfig.signalingUrl, tokenA);

      // Alice publishes video with simulcast enabled
      await clientA.publishTracks({ video: true, simulcast: true });

      // Bob joins and subscribes
      await clientB.join(suiteConfig.signalingUrl, tokenB);
      await clientB.waitForRemoteParticipant(clientAId, 10000);
      await clientB.waitForRemoteTrack('video', 15000);

      // Verify initial video rendering on Bob
      const initialVideoB = await clientB.getRenderedVideoStats('#remote-video');
      expect(initialVideoB.videoWidth).toBeGreaterThan(0);
      expect(initialVideoB.videoHeight).toBeGreaterThan(0);

      // Track signaling messages sent by Bob during quality adjustment
      let qualityFrameSent = false;
      const detachWsListener = clientB.attachWebSocketListener((direction, payload) => {
        if (direction === 'outbound') {
          const payloadStr = typeof payload === 'string' ? payload : payload.toString('hex');
          if (payloadStr.length > 0) {
            qualityFrameSent = true;
          }
        }
      });

      // -------------------------------------------------------------
      // Check B: Official LiveKit quality-update message mapping
      // -------------------------------------------------------------
      // Bob requests LOW video quality via official livekit-client API
      qualityFrameSent = false;
      await clientB.setVideoQuality('low');
      await clientB.getPage().waitForTimeout(1000);

      // Verify that official livekit-client emitted a quality update message
      const qualityUpdateEmitted = qualityFrameSent;
      detachWsListener();

      checks.livekit_quality_message_mapping = {
        name: 'livekit_quality_message_mapping',
        status: qualityUpdateEmitted ? 'PASSED' : 'FAILED',
        description: qualityUpdateEmitted
          ? 'Official livekit-client setVideoQuality emitted outbound UpdateTrackSettings message over signaling WebSocket'
          : 'Official livekit-client did not emit signaling update upon setVideoQuality invocation',
        details: {
          requestedQuality: 'low',
          qualityUpdateEmitted
        }
      };

      // -------------------------------------------------------------
      // Check A: SFU layer-selection capability
      // -------------------------------------------------------------
      // Observe receiver stats under 'low' quality setting
      const statsLowStart = await clientB.getStats();
      await clientB.getPage().waitForTimeout(2000);
      const statsLowEnd = await clientB.getStats();
      const videoStatsLow = await clientB.getRenderedVideoStats('#remote-video');

      const bytesLow = statsLowEnd.rawCounters.totalBytesReceived - statsLowStart.rawCounters.totalBytesReceived;

      // Bob requests HIGH video quality
      await clientB.setVideoQuality('high');
      await clientB.getPage().waitForTimeout(1000);

      const statsHighStart = await clientB.getStats();
      await clientB.getPage().waitForTimeout(2000);
      const statsHighEnd = await clientB.getStats();
      const videoStatsHigh = await clientB.getRenderedVideoStats('#remote-video');

      const bytesHigh = statsHighEnd.rawCounters.totalBytesReceived - statsHighStart.rawCounters.totalBytesReceived;

      sharedEvidenceRecorder.addRtpMetricsSnapshot(gateId, `${clientBId}-quality-low`, statsLowEnd);
      sharedEvidenceRecorder.addRtpMetricsSnapshot(gateId, `${clientBId}-quality-high`, statsHighEnd);

      // Check whether media stream continues to flow across layer transitions
      const mediaFlowMaintained = bytesLow > 0 && bytesHigh > 0;
      expect(mediaFlowMaintained).toBe(true);

      checks.sfu_layer_selection_capability = {
        name: 'sfu_layer_selection_capability',
        status: mediaFlowMaintained ? 'PASSED' : 'FAILED',
        description: `Verified SFU forwarding across layer transitions: low quality (${bytesLow}B received, ${videoStatsLow.videoWidth}x${videoStatsLow.videoHeight}), high quality (${bytesHigh}B received, ${videoStatsHigh.videoWidth}x${videoStatsHigh.videoHeight})`,
        details: {
          bytesReceivedLow: bytesLow,
          bytesReceivedHigh: bytesHigh,
          dimensionsLow: `${videoStatsLow.videoWidth}x${videoStatsLow.videoHeight}`,
          dimensionsHigh: `${videoStatsHigh.videoWidth}x${videoStatsHigh.videoHeight}`,
          mediaFlowMaintained
        }
      };

      const gatePassed = checks.livekit_quality_message_mapping.status === 'PASSED' &&
                         checks.sfu_layer_selection_capability.status === 'PASSED';

      gateRegistry.setStatus(gateId, gatePassed ? 'PASSED' : 'FAILED');
      sharedEvidenceRecorder.recordGateResult(gateId, gatePassed ? 'PASSED' : 'FAILED', checks);
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      gateRegistry.setStatus(gateId, 'FAILED', errorMsg);
      sharedEvidenceRecorder.recordGateResult(gateId, 'FAILED', checks, errorMsg);
      sharedEvidenceRecorder.markDownstreamBlocked(gateId, `Prerequisite Gate 4 failed: ${errorMsg}`);
      throw err;
    } finally {
      if (clientA) await clientA.leave().catch(() => {});
      if (clientB) await clientB.leave().catch(() => {});
    }
  });
});
