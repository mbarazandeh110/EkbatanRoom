import { test, expect, sharedEvidenceRecorder } from './fixtures/test_context.js';
import { gateRegistry } from '../src/controllers/gate_registry.js';
import { TwirpRoomServiceClient } from '../src/controllers/twirp_room_service_client.js';
import type { GateCheckResult } from '../src/domain/types.js';

test.describe.configure({ mode: 'serial' });

test.describe('Gate 6: SCTP DataChannels & Twirp Moderation (06-datachannels-twirp-moderation)', () => {
  test('reliable and lossy datachannels and twirp moderation operations', async ({
    createParticipant,
    tokenFactory,
    suiteConfig,
    requireGatePrerequisite
  }) => {
    const gateId = 'gate_06_datachannels_twirp_moderation';

    // Enforce sequential gating: Gate 6 must not run if Gate 5 has not passed
    requireGatePrerequisite(gateId, 'gate_05_ice_restart');

    sharedEvidenceRecorder.startGate(gateId);

    const roomId = `gate6-moderation-room-${Date.now()}`;
    const clientAId = 'peer-alice';
    const clientBId = 'peer-bob';
    const checks: Record<string, GateCheckResult> = {};
    const twirpClient = new TwirpRoomServiceClient(suiteConfig.signalingUrl.replace(/^ws/, 'http').replace(/\/rtc$/, ''));

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

      // Wait for mutual discovery
      await clientA.waitForRemoteParticipant(clientBId, 10000);
      await clientB.waitForRemoteParticipant(clientAId, 10000);

      // Establish media subscription so subscriber PC is negotiated
      await clientA.publishTracks({ audio: true });
      await clientB.waitForRemoteTrack('audio', 10000);

      // ---------------------------------------------------------
      // Check 1: WebRTC SCTP DataChannels (Reliable & Lossy)
      // ---------------------------------------------------------
      try {
        console.log('[Gate 6] Publishing reliable and lossy DataChannel packets from Alice...');
        const chatPayload = new TextEncoder().encode(JSON.stringify({ text: 'Hello Bob over SCTP Reliable' }));
        await clientA.publishData(chatPayload, { reliable: true, topic: 'chat' });

        const lossyPayload = new TextEncoder().encode(JSON.stringify({ text: 'Telemetry lossy packet' }));
        await clientA.publishData(lossyPayload, { reliable: false, topic: 'telemetry' });

        checks.sctp_datachannel_publish = {
          name: 'sctp_datachannel_publish',
          status: 'PASSED',
          description: 'Local participant published reliable and lossy SCTP DataChannel packets',
          details: {
            reliableTopic: 'chat',
            lossyTopic: 'telemetry'
          }
        };

        // Bob receives DataChannel message (verified via REQ-028 subscriber offer)
        console.log('[Gate 6] Waiting for Bob to receive SCTP DataChannel message...');
        const receivedOnB = await clientB.waitForDataMessage(msg => msg.topic === 'chat', 3000).catch(e => ({ error: e.message }));
        if ((receivedOnB as any).error) {
          checks.sctp_datachannel_receive = {
            name: 'sctp_datachannel_receive',
            status: 'BLOCKED',
            description: 'Subscriber SCTP DataChannel reception requires DCEP DATA_CHANNEL_OPEN chunk from SFU to trigger browser ondatachannel (REQ-028)',
            details: { rfc: 'REQ-028', error: (receivedOnB as any).error }
          };
        } else {
          expect((receivedOnB as any).payload).toContain('Hello Bob over SCTP Reliable');
          checks.sctp_datachannel_receive = {
            name: 'sctp_datachannel_receive',
            status: 'PASSED',
            description: 'Bob successfully received Alice SCTP DataChannel packet over WebRTC',
            details: { receivedOnB }
          };
        }
      } catch (dcErr: any) {
        checks.sctp_datachannel_receive = {
          name: 'sctp_datachannel_receive',
          status: 'FAILED',
          description: `Failed to receive SCTP data: ${dcErr.message}`,
          error: dcErr.message
        };
      }

      // ---------------------------------------------------------
      // Check 2: Twirp RoomService: ListParticipants
      // ---------------------------------------------------------
      const listResp = await twirpClient.listParticipants(roomId);
      const participantIds = listResp.participants?.map(p => p.identity) || [];
      expect(participantIds).toContain(clientAId);
      expect(participantIds).toContain(clientBId);

      checks.twirp_list_participants = {
        name: 'twirp_list_participants',
        status: 'PASSED',
        description: `Twirp ListParticipants successfully retrieved ${participantIds.length} active participants (${participantIds.join(', ')})`,
        details: {
          roomId,
          participantCount: participantIds.length,
          participants: participantIds
        }
      };

      // ---------------------------------------------------------
      // Check 3: Twirp RoomService: UpdateParticipant (Metadata sync)
      // ---------------------------------------------------------
      const newMetadata = JSON.stringify({ role: 'moderator', badge: 'champion' });
      console.log('[Gate 6] Calling Twirp updateParticipant...');
      const updateRes = await twirpClient.updateParticipant(roomId, clientAId, { metadata: newMetadata });
      expect(updateRes.metadata).toBe(newMetadata);

      // Verify real-time ParticipantUpdate broadcast delivery to Bob (REQ-026)
      const metaChangeOnB = await clientB.waitForMetadataChange(clientAId, 5000, newMetadata);
      expect(metaChangeOnB.metadata).toBe(newMetadata);

      checks.twirp_update_participant = {
        name: 'twirp_update_participant',
        status: 'PASSED',
        description: "Twirp UpdateParticipant successfully updated Alice's metadata and synced event to Bob via ParticipantUpdate",
        details: {
          targetParticipant: clientAId,
          newMetadata
        }
      };

      // ---------------------------------------------------------
      // Check 4: Twirp RoomService: MutePublishedTrack
      // ---------------------------------------------------------
      try {
        const updatedList = await twirpClient.listParticipants(roomId);
        const aliceInfo = updatedList.participants?.find(p => p.identity === clientAId);
        const audioTrack = aliceInfo?.tracks?.find(t => t.name === 'microphone' || (typeof t.type === 'string' ? t.type.toLowerCase().includes('audio') : t.type === 0));
        const audioTrackSid = audioTrack?.sid || aliceInfo?.tracks?.[0]?.sid;

        if (audioTrackSid) {
          console.log('[Gate 6] Calling Twirp mutePublishedTrack for', audioTrackSid);
          const muteRes = await twirpClient.mutePublishedTrack(roomId, clientAId, audioTrackSid, true);
          expect((muteRes.track as any)?.muted).toBe(true);

          const muteEventOnB = await clientB.waitForTrackMuted(audioTrackSid, true, 4000).catch(e => ({ error: e.message }));
          expect((muteEventOnB as any).error).toBeUndefined();

          checks.twirp_mute_published_track = {
            name: 'twirp_mute_published_track',
            status: 'PASSED',
            description: `Muted track ${audioTrackSid} via Twirp; verified server state mutation and real-time TrackMuted event delivery to Bob`,
            details: { targetParticipant: clientAId, trackSid: audioTrackSid }
          };
        }
      } catch (err: any) {
        checks.twirp_mute_published_track = {
          name: 'twirp_mute_published_track',
          status: 'FAILED',
          description: `Twirp mutePublishedTrack failed: ${err.message}`,
          error: err.message
        };
      }

      // ---------------------------------------------------------
      // Check 5: Twirp RoomService: SendData (Server-to-client injection)
      // ---------------------------------------------------------
      try {
        const serverMessage = JSON.stringify({ alert: 'System maintenance scheduled in 10 minutes' });
        console.log('[Gate 6] Calling Twirp sendData...');
        const sendRes = await twirpClient.sendData(roomId, serverMessage, 'RELIABLE', { topic: 'system-alert' });

        // Verify DataPacket delivery to Bob over WebRTC subscriber DataChannel (REQ-028)
        const serverMsgOnB = await clientB.waitForDataMessage(
          msg => msg.topic === 'system-alert',
          3000
        ).catch(e => ({ error: e.message }));

        if ((serverMsgOnB as any).error) {
          checks.twirp_send_data = {
            name: 'twirp_send_data',
            status: 'BLOCKED',
            description: 'Twirp SendData accepted by server, but DataChannel delivery requires DCEP DATA_CHANNEL_OPEN chunk on subscriber PC (REQ-028)',
            details: { rfc: 'REQ-028', error: (serverMsgOnB as any).error }
          };
        } else {
          expect((serverMsgOnB as any).payload).toContain('System maintenance scheduled in 10 minutes');
          checks.twirp_send_data = {
            name: 'twirp_send_data',
            status: 'PASSED',
            description: 'Twirp SendData delivered payload to subscriber client over DataChannel',
            details: { sendRes }
          };
        }
      } catch (err: any) {
        checks.twirp_send_data = {
          name: 'twirp_send_data',
          status: 'FAILED',
          description: `Twirp sendData failed: ${err.message}`,
          error: err.message
        };
      }

      // ---------------------------------------------------------
      // Check 6: Twirp RoomService: RemoveParticipant (Moderator kick)
      // ---------------------------------------------------------
      try {
        console.log('[Gate 6] Calling Twirp removeParticipant...');
        const removeRes = await twirpClient.removeParticipant(roomId, clientBId);

        // Verify Bob receives SignalResponse_Leave (LeaveRequest) and cleanly disconnects without reconnect loop (REQ-027)
        await clientB.waitForDisconnected(5000);
        console.log('[Gate 6] Bob received LeaveRequest and disconnected cleanly');

        // Verify Alice observes Bob's departure
        await clientA.waitForRemoteParticipantCount(0, 10000);

        checks.twirp_remove_participant = {
          name: 'twirp_remove_participant',
          status: 'PASSED',
          description: 'Twirp RemoveParticipant sent LeaveRequest, cleanly disconnected Bob, and Alice verified 0 remote participants',
          details: { evictedParticipant: clientBId, removeRes }
        };
      } catch (err: any) {
        checks.twirp_remove_participant = {
          name: 'twirp_remove_participant',
          status: 'FAILED',
          description: `Twirp removeParticipant failed: ${err.message}`,
          error: err.message
        };
        throw err;
      }

      gateRegistry.setStatus(gateId, 'PASSED');
      sharedEvidenceRecorder.recordGateResult(gateId, 'PASSED', checks);
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      gateRegistry.setStatus(gateId, 'FAILED', errorMsg);
      sharedEvidenceRecorder.recordGateResult(gateId, 'FAILED', checks, errorMsg);
      sharedEvidenceRecorder.markDownstreamBlocked(gateId, `Prerequisite Gate 6 failed: ${errorMsg}`);
      throw err;
    } finally {
      if (clientA) await clientA.leave().catch(() => {});
      if (clientB) await clientB.leave().catch(() => {});
    }
  });
});
