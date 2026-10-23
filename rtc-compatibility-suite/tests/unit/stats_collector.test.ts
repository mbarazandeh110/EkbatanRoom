import { describe, it, expect } from 'vitest';
import { StatsCollector, type RawStatsReportEntry } from '../../src/controllers/stats_collector.js';

describe('StatsCollector', () => {
  it('parses outbound and inbound RTP entries and calculates aggregate byte counters', () => {
    const rawReports: RawStatsReportEntry[] = [
      {
        type: 'outbound-rtp',
        kind: 'audio',
        ssrc: 1001,
        bytesSent: 15400,
        packetsSent: 120
      },
      {
        type: 'outbound-rtp',
        kind: 'video',
        ssrc: 1002,
        bytesSent: 82500,
        packetsSent: 350,
        frameWidth: 1280,
        frameHeight: 720,
        framesSent: 60
      },
      {
        type: 'inbound-rtp',
        kind: 'video',
        ssrc: 2002,
        bytesReceived: 81200,
        packetsReceived: 348,
        packetsLost: 2,
        jitter: 0.005,
        frameWidth: 1280,
        frameHeight: 720,
        framesDecoded: 59,
        framesDropped: 1,
        pliCount: 1,
        nackCount: 2
      },
      {
        type: 'candidate-pair',
        state: 'succeeded',
        nominated: true,
        currentRoundTripTime: 0.012,
        availableOutgoingBitrate: 2500000
      }
    ];

    const snapshot = StatsCollector.parseStatsReport(rawReports, 'connected', 'connected', 'stable');

    expect(snapshot.rawCounters.totalBytesSent).toBe(15400 + 82500);
    expect(snapshot.rawCounters.totalBytesReceived).toBe(81200);

    expect(snapshot.audioOutbound).toBeDefined();
    expect(snapshot.audioOutbound?.bytesSent).toBe(15400);

    expect(snapshot.videoOutbound).toBeDefined();
    expect(snapshot.videoOutbound?.frameWidth).toBe(1280);

    expect(snapshot.videoInbound).toBeDefined();
    expect(snapshot.videoInbound?.framesDecoded).toBe(59);
    expect(snapshot.videoInbound?.packetsLost).toBe(2);
    expect(snapshot.videoInbound?.nackCount).toBe(2);

    expect(snapshot.activeCandidatePair).toBeDefined();
    expect(snapshot.activeCandidatePair?.currentRoundTripTimeMs).toBeCloseTo(12, 1);
    expect(snapshot.activeCandidatePair?.availableOutgoingBitrate).toBe(2500000);
  });
});
