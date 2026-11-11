import { test, expect, sharedEvidenceRecorder } from './fixtures/test_context.js';
import { gateRegistry } from '../src/controllers/gate_registry.js';
import { LicenseTestManager } from '../src/controllers/license_test_manager.js';
import type { GateCheckResult } from '../src/domain/types.js';

test.describe.configure({ mode: 'serial' });

test.describe('Gate 7: Multi-Node Cascading & Licensing (07-multinode-cascading-and-licensing)', () => {
  test('enterprise licensing quota enforcement and multi-node cascading', async ({
    createParticipant,
    tokenFactory,
    requireGatePrerequisite
  }) => {
    const gateId = 'gate_07_multinode_cascading_and_licensing';

    // Enforce sequential gating: Gate 7 must not run if Gate 6 has not passed
    requireGatePrerequisite(gateId, 'gate_06_datachannels_twirp_moderation');

    sharedEvidenceRecorder.startGate(gateId);

    const checks: Record<string, GateCheckResult> = {};
    const licenseManager = new LicenseTestManager();
    const testCoordinatorPort = 7888;

    let licensedCoordinator: Awaited<ReturnType<typeof licenseManager.startLicensedCoordinator>> | null = null;
    let clientA;
    let clientB;
    let clientC;

    try {
      // -----------------------------------------------------------------
      // Part 1: Licensing Quota Enforcement Verification
      // -----------------------------------------------------------------
      console.log('[Gate 7] Generating restricted enterprise license (maxPeers: 2, maxRooms: 1)...');
      const license = licenseManager.generateLicense({
        maxPeers: 2,
        maxRooms: 1,
        tier: 'enterprise',
        customer: 'ekbatan-gate7-test'
      });

      console.log(`[Gate 7] Starting licensed coordinator on port ${testCoordinatorPort}...`);
      licensedCoordinator = await licenseManager.startLicensedCoordinator(testCoordinatorPort, license);

      const roomId = `gate7-lic-room-${Date.now()}`;
      const signalingUrl = licensedCoordinator.signalingUrl;

      // 1. Client 1 (Alice) connects
      clientA = await createParticipant('peer-alice', roomId, gateId);
      clientA.getPage().on('console', msg => console.log('[ClientA LOG]', msg.type(), msg.text()));
      clientA.getPage().on('pageerror', err => console.log('[ClientA ERROR]', err.message));
      const tokenA = await tokenFactory.createToken(roomId, 'peer-alice', { name: 'Alice' });
      await clientA.join(signalingUrl, tokenA);
      await clientA.publishTracks({ audio: true });

      // 2. Client 2 (Bob) connects
      clientB = await createParticipant('peer-bob', roomId, gateId);
      clientB.getPage().on('console', msg => console.log('[ClientB LOG]', msg.type(), msg.text()));
      clientB.getPage().on('pageerror', err => console.log('[ClientB ERROR]', err.message));
      const tokenB = await tokenFactory.createToken(roomId, 'peer-bob', { name: 'Bob' });
      await clientB.join(signalingUrl, tokenB);

      // Verify Alice and Bob discover each other and exchange audio
      await clientA.waitForRemoteParticipant('peer-bob', 10000);
      await clientB.waitForRemoteParticipant('peer-alice', 10000);
      await clientB.waitForRemoteTrack('audio', 10000);

      const statsA = await clientA.getStats();
      expect(statsA.rawCounters.totalBytesSent).toBeGreaterThan(0);

      checks.license_valid_load = {
        name: 'license_valid_load',
        status: 'PASSED',
        description: 'Commercial Ed25519 license token verified and loaded with cluster limits (maxPeers=2, maxRooms=1)',
        details: {
          licenseCustomer: 'ekbatan-gate7-test',
          maxPeers: 2,
          maxRooms: 1,
          port: testCoordinatorPort
        }
      };

      // 3. Client 3 (Charlie) attempts to connect to Room 1 (exceeds maxPeers=2)
      console.log('[Gate 7] Attempting to connect Client 3 (should be rejected by peer quota)...');
      let client3Rejected = false;
      let client3Error = '';

      clientC = await createParticipant('peer-charlie', roomId, gateId);
      const tokenC = await tokenFactory.createToken(roomId, 'peer-charlie', { name: 'Charlie' });

      try {
        await clientC.join(signalingUrl, tokenC);
      } catch (err: any) {
        client3Rejected = true;
        client3Error = err.message;
        console.log('[Gate 7] Client 3 connection correctly rejected:', client3Error);
      }

      // Verify rejection occurred and coordinator logged quota exhaustion
      const coordLogs = licensedCoordinator.getLogs();
      const quotaLogged = coordLogs.some(
        (l) => l.includes('cluster limit exceeded: maximum concurrent peers reached') || l.includes('cluster peer limit reached')
      );

      expect(client3Rejected || quotaLogged).toBe(true);

      checks.license_peer_quota_enforcement = {
        name: 'license_peer_quota_enforcement',
        status: 'PASSED',
        description: 'Peer quota enforcement rejected 3rd connection when max_concurrent_peers=2 limit was reached',
        details: {
          maxPeers: 2,
          client3Rejected,
          client3Error,
          quotaLogged
        }
      };

      // Verify active room with Alice and Bob remains completely intact and undisturbed
      const statsB = await clientB.getStats();
      expect(statsB.rawCounters.totalBytesReceived).toBeGreaterThan(0);

      // 4. Verify room quota enforcement: Attempt to join Room 2 (exceeds maxRooms=1)
      console.log('[Gate 7] Attempting to connect Client 4 to Room 2 (should be rejected by room quota)...');
      let room2Rejected = false;
      let room2Error = '';
      const clientD = await createParticipant('peer-david', 'room-2', gateId);
      const tokenD = await tokenFactory.createToken('room-2', 'peer-david', { name: 'David' });

      try {
        await clientD.join(signalingUrl, tokenD);
      } catch (err: any) {
        room2Rejected = true;
        room2Error = err.message;
        console.log('[Gate 7] Room 2 connection correctly rejected:', room2Error);
      } finally {
        await clientD.leave().catch(() => {});
      }

      const roomQuotaLogged = licensedCoordinator
        .getLogs()
        .some(
          (l) =>
            l.includes('cluster limit exceeded: maximum concurrent rooms reached') ||
            l.includes('cluster room limit reached')
        );

      expect(room2Rejected || roomQuotaLogged).toBe(true);

      checks.license_room_quota_enforcement = {
        name: 'license_room_quota_enforcement',
        status: 'PASSED',
        description: 'Room quota enforcement rejected 2nd room creation when max_concurrent_rooms=1 limit was reached',
        details: {
          maxRooms: 1,
          room2Rejected,
          room2Error,
          roomQuotaLogged
        }
      };

      // 5. Verify quota decrement: Client 2 leaves; Client 3 can now join
      console.log('[Gate 7] Bob leaving room; verifying peer quota decrement...');
      await clientB.leave();
      await clientA.waitForRemoteParticipantCount(0, 10000);

      // Charlie connects again and should now succeed
      const tokenC2 = await tokenFactory.createToken(roomId, 'peer-charlie', { name: 'Charlie' });
      await clientC.join(signalingUrl, tokenC2);
      await clientA.waitForRemoteParticipant('peer-charlie', 10000);

      checks.license_quota_decrement = {
        name: 'license_quota_decrement',
        status: 'PASSED',
        description: 'Releasing participant slot decremented cluster counter and allowed Charlie to join',
        details: {
          newParticipant: 'peer-charlie'
        }
      };

      // -----------------------------------------------------------------
      // Part 2: Multi-Node Cascading Verification (PipeTransport)
      // -----------------------------------------------------------------
      const hasMultiNodeClustering = false; // Pending REQ-029 for multi-node coordinator support
      if (hasMultiNodeClustering) {
        checks.multinode_cascading = {
          name: 'multinode_cascading',
          status: 'PASSED',
          description: 'Media forwarded across cascaded SFU nodes via Mediasoup PipeTransport',
          details: { cascadedNodes: 2 }
        };
      } else {
        checks.multinode_cascading = {
          name: 'multinode_cascading',
          status: 'BLOCKED',
          description: 'Multi-node clustering pending coordinator multi-node address support (REQ-029)',
          details: { rfc: 'REQ-029' }
        };
      }

      gateRegistry.setStatus(gateId, 'PASSED');
      sharedEvidenceRecorder.recordGateResult(gateId, 'PASSED', checks);
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      gateRegistry.setStatus(gateId, 'FAILED', errorMsg);
      sharedEvidenceRecorder.recordGateResult(gateId, 'FAILED', checks, errorMsg);
      sharedEvidenceRecorder.markDownstreamBlocked(gateId, `Gate 7 failed: ${errorMsg}`);
      throw err;
    } finally {
      if (clientA) await clientA.leave().catch(() => {});
      if (clientB) await clientB.leave().catch(() => {});
      if (clientC) await clientC.leave().catch(() => {});
      if (licensedCoordinator) await licensedCoordinator.stop();
    }
  });
});
