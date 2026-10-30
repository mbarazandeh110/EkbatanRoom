import { test, expect, sharedEvidenceRecorder } from './fixtures/test_context.js';
import { gateRegistry } from '../src/controllers/gate_registry.js';
import type { GateCheckResult } from '../src/domain/types.js';

test.describe.configure({ mode: 'serial' });

test.describe('Gate 2: Bidirectional Media RTP & Video Rendering (02-bidirectional-rtp)', () => {
  test('two independent clients publish and subscribe bidirectional audio and video', async ({
    createParticipant,
    tokenFactory,
    suiteConfig,
    requireGatePrerequisite
  }) => {
    const gateId = 'gate_02_bidirectional_rtp';

    // Enforce sequential gating: Gate 2 must not run if Gate 1 has not passed
    requireGatePrerequisite(gateId, 'gate_01_connection');

    sharedEvidenceRecorder.startGate(gateId);

    const roomId = `gate2-media-room-${Date.now()}`;
    const clientAId = 'peer-alice';
    const clientBId = 'peer-bob';
    const checks: Record<string, GateCheckResult> = {};

    let clientA;
    let clientB;

    try {
      // 1. Create Client A and join
      clientA = await createParticipant(clientAId, roomId, gateId);
      clientA.getPage().on('console', msg => console.log('[ClientA LOG]', msg.type(), msg.text()));
      clientA.getPage().on('pageerror', err => console.log('[ClientA ERROR]', err.message));
      const tokenA = await tokenFactory.createToken(roomId, clientAId, { name: 'Alice' });
      await clientA.join(suiteConfig.signalingUrl, tokenA);

      // 2. Create Client B and join
      clientB = await createParticipant(clientBId, roomId, gateId);
      clientB.getPage().on('console', msg => console.log('[ClientB LOG]', msg.type(), msg.text()));
      clientB.getPage().on('pageerror', err => console.log('[ClientB ERROR]', err.message));
      const tokenB = await tokenFactory.createToken(roomId, clientBId, { name: 'Bob' });
      await clientB.join(suiteConfig.signalingUrl, tokenB);

      // Wait for participants to discover each other
      await clientA.waitForRemoteParticipant(clientBId, 10000);
      await clientB.waitForRemoteParticipant(clientAId, 10000);

      // ---------------------------------------------------------
      // Step A: Forward Direction (Client A Publishes -> Client B Subscribes)
      // ---------------------------------------------------------
      await clientA.publishTracks({ audio: true, video: true });

      for (let i = 0; i < 4; i++) {
        await clientB.getPage().waitForTimeout(1000);
        if (i === 0) {
          const sdpsA = await clientA.getPage().evaluate(() => {
            const room = (window.testHarness as any)?.room;
            const pub = room?.engine?.pcManager?.publisher;
            const pc: RTCPeerConnection | undefined = pub?.pc ?? pub;
            return {
              pubOffer: pc?.localDescription?.sdp,
              pubAnswer: pc?.remoteDescription?.sdp,
            };
          });
          const sdpsB = await clientB.getPage().evaluate(() => {
            const room = (window.testHarness as any)?.room;
            const sub = room?.engine?.pcManager?.subscriber;
            const pc: RTCPeerConnection | undefined = sub?.pc ?? sub;
            return {
              subOffer: pc?.remoteDescription?.sdp,
              subAnswer: pc?.localDescription?.sdp,
            };
          });
          console.log('[SDP Alice Pub Offer]\n', sdpsA.pubOffer);
          console.log('[SDP Alice Pub Answer]\n', sdpsA.pubAnswer);
          console.log('[SDP Bob Sub Offer]\n', sdpsB.subOffer);
          console.log('[SDP Bob Sub Answer]\n', sdpsB.subAnswer);
        }
        const statsA = await clientA.getPage().evaluate(async () => {
          const room = (window.testHarness as unknown as { room: { engine: { pcManager?: { publisher?: { getStats?: () => Promise<RTCStatsReport> } } } } }).room;
          const pubPc = room?.engine?.pcManager?.publisher;
          const out: Record<string, unknown>[] = [];
          if (pubPc?.getStats) {
            const reports = await pubPc.getStats();
            reports.forEach((r) => {
              if (r.type === 'outbound-rtp') {
                out.push({
                  kind: r.kind,
                  ssrc: r.ssrc,
                  bytesSent: r.bytesSent,
                  packetsSent: r.packetsSent,
                  framesEncoded: r.framesEncoded,
                  keyFramesEncoded: (r as unknown as { keyFramesEncoded?: number }).keyFramesEncoded,
                  pliCount: (r as unknown as { pliCount?: number }).pliCount,
                  firCount: (r as unknown as { firCount?: number }).firCount,
                  nackCount: (r as unknown as { nackCount?: number }).nackCount,
                  codec: (r as any).codecId ? reports.get((r as any).codecId) : undefined
                });
              }
            });
          }
          return out;
        });

        const statsB = await clientB.getPage().evaluate(async () => {
          const room = (window.testHarness as unknown as { room: { engine: { pcManager?: { subscriber?: { getStats?: () => Promise<RTCStatsReport> } } } } }).room;
          const subPc = room?.engine?.pcManager?.subscriber;
          const inb: Record<string, unknown>[] = [];
          if (subPc?.getStats) {
            const reports = await subPc.getStats();
            reports.forEach((r) => {
              if (r.type === 'inbound-rtp') {
                inb.push({
                  kind: r.kind,
                  ssrc: r.ssrc,
                  bytesReceived: r.bytesReceived,
                  packetsReceived: r.packetsReceived,
                  framesDecoded: (r as unknown as { framesDecoded?: number }).framesDecoded,
                  keyFramesDecoded: (r as unknown as { keyFramesDecoded?: number }).keyFramesDecoded,
                  pliCount: (r as unknown as { pliCount?: number }).pliCount,
                  firCount: (r as unknown as { firCount?: number }).firCount,
                  nackCount: (r as unknown as { nackCount?: number }).nackCount
                });
              }
            });
          }
          const videoEl = document.querySelector('#remote-video') as HTMLVideoElement | null;
          return {
            inbound: inb,
            videoEl: {
              srcObject: !!videoEl?.srcObject,
              readyState: videoEl?.readyState,
              videoWidth: videoEl?.videoWidth,
              videoHeight: videoEl?.videoHeight
            }
          };
        });

        console.log(`[RTCP Check +${i + 1}s] Alice Outbound:`, JSON.stringify(statsA));
        console.log(`[RTCP Check +${i + 1}s] Bob Inbound:`, JSON.stringify(statsB));
        if ((statsB.videoEl.videoWidth ?? 0) > 0) break;
      }

      // Client B waits for remote track to be attached and rendered
      await clientB.waitForRemoteTrack('video', 15000);

      // Check rendered dimensions on Client B
      const initialVideoB = await clientB.getRenderedVideoStats('#remote-video');
      expect(initialVideoB.videoWidth).toBeGreaterThan(0);
      expect(initialVideoB.videoHeight).toBeGreaterThan(0);

      // Check frame progression on Client B
      await clientB.getPage().waitForTimeout(1000);
      const progressedVideoB = await clientB.getRenderedVideoStats('#remote-video');
      expect(progressedVideoB.videoWidth).toBe(initialVideoB.videoWidth);
      expect(progressedVideoB.currentTime).toBeGreaterThanOrEqual(initialVideoB.currentTime);

      // Verify WebRTC getStats delta for Forward Direction
      const statsA_start = await clientA.getStats();
      const statsB_start = await clientB.getStats();

      await clientA.getPage().waitForTimeout(1500);

      const statsA_end = await clientA.getStats();
      const statsB_end = await clientB.getStats();

      const bytesSentA = statsA_end.rawCounters.totalBytesSent - statsA_start.rawCounters.totalBytesSent;
      const bytesReceivedB = statsB_end.rawCounters.totalBytesReceived - statsB_start.rawCounters.totalBytesReceived;

      sharedEvidenceRecorder.addRtpMetricsSnapshot(gateId, `${clientAId}-forward`, statsA_end);
      sharedEvidenceRecorder.addRtpMetricsSnapshot(gateId, `${clientBId}-forward`, statsB_end);

      expect(statsA_end.rawCounters.totalBytesSent).toBeGreaterThan(0);
      expect(statsB_end.rawCounters.totalBytesReceived).toBeGreaterThan(0);

      checks.forward_media_rendering = {
        name: 'forward_media_rendering',
        status: 'PASSED',
        description: `Client B rendered video with dimensions ${progressedVideoB.videoWidth}x${progressedVideoB.videoHeight}`,
        details: {
          width: progressedVideoB.videoWidth,
          height: progressedVideoB.videoHeight,
          readyState: progressedVideoB.readyState
        }
      };

      checks.forward_rtp_bytes = {
        name: 'forward_rtp_bytes',
        status: 'PASSED',
        description: `Verified forward RTP transfer: Client A sent ${bytesSentA} bytes delta, Client B received ${bytesReceivedB} bytes delta`,
        details: {
          bytesSentDelta: bytesSentA,
          bytesReceivedDelta: bytesReceivedB,
          totalSent: statsA_end.rawCounters.totalBytesSent,
          totalReceived: statsB_end.rawCounters.totalBytesReceived
        }
      };

      // ---------------------------------------------------------
      // Step B: Reverse Direction (Client B Publishes -> Client A Subscribes)
      // ---------------------------------------------------------
      await clientB.publishTracks({ audio: true, video: true });

      // Client A waits for remote track to be attached and rendered
      await clientA.waitForRemoteTrack('video', 15000);

      const initialVideoA = await clientA.getRenderedVideoStats('#remote-video');
      expect(initialVideoA.videoWidth).toBeGreaterThan(0);
      expect(initialVideoA.videoHeight).toBeGreaterThan(0);

      await clientA.getPage().waitForTimeout(1000);
      const progressedVideoA = await clientA.getRenderedVideoStats('#remote-video');
      expect(progressedVideoA.currentTime).toBeGreaterThanOrEqual(initialVideoA.currentTime);

      const statsB_revStart = await clientB.getStats();
      const statsA_revStart = await clientA.getStats();

      await clientB.getPage().waitForTimeout(1500);

      const statsB_revEnd = await clientB.getStats();
      const statsA_revEnd = await clientA.getStats();

      const bytesSentB = statsB_revEnd.rawCounters.totalBytesSent - statsB_revStart.rawCounters.totalBytesSent;
      const bytesReceivedA = statsA_revEnd.rawCounters.totalBytesReceived - statsA_revStart.rawCounters.totalBytesReceived;

      sharedEvidenceRecorder.addRtpMetricsSnapshot(gateId, `${clientBId}-reverse`, statsB_revEnd);
      sharedEvidenceRecorder.addRtpMetricsSnapshot(gateId, `${clientAId}-reverse`, statsA_revEnd);

      expect(statsB_revEnd.rawCounters.totalBytesSent).toBeGreaterThan(0);
      expect(statsA_revEnd.rawCounters.totalBytesReceived).toBeGreaterThan(0);

      checks.reverse_media_rendering = {
        name: 'reverse_media_rendering',
        status: 'PASSED',
        description: `Client A rendered reverse video with dimensions ${progressedVideoA.videoWidth}x${progressedVideoA.videoHeight}`,
        details: {
          width: progressedVideoA.videoWidth,
          height: progressedVideoA.videoHeight
        }
      };

      checks.reverse_rtp_bytes = {
        name: 'reverse_rtp_bytes',
        status: 'PASSED',
        description: `Verified reverse RTP transfer: Client B sent ${bytesSentB} bytes delta, Client A received ${bytesReceivedA} bytes delta`,
        details: {
          bytesSentDelta: bytesSentB,
          bytesReceivedDelta: bytesReceivedA,
          totalSent: statsB_revEnd.rawCounters.totalBytesSent,
          totalReceived: statsA_revEnd.rawCounters.totalBytesReceived
        }
      };

      gateRegistry.setStatus(gateId, 'PASSED');
      sharedEvidenceRecorder.recordGateResult(gateId, 'PASSED', checks);
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      gateRegistry.setStatus(gateId, 'FAILED', errorMsg);
      sharedEvidenceRecorder.recordGateResult(gateId, 'FAILED', checks, errorMsg);
      sharedEvidenceRecorder.markDownstreamBlocked(gateId, `Prerequisite Gate 2 failed: ${errorMsg}`);
      throw err;
    } finally {
      if (clientA) await clientA.leave().catch(() => {});
      if (clientB) await clientB.leave().catch(() => {});
    }
  });
});
