import type { RtpMetricsSnapshot, RtpStreamMetrics, CandidatePairMetrics } from '../domain/qoe_metrics.js';

export interface RawStatsReportEntry {
  type: string;
  kind?: string;
  mediaType?: string;
  ssrc?: number;
  bytesSent?: number;
  bytesReceived?: number;
  packetsSent?: number;
  packetsReceived?: number;
  packetsLost?: number;
  jitter?: number;
  frameWidth?: number;
  frameHeight?: number;
  framesSent?: number;
  framesReceived?: number;
  framesDecoded?: number;
  framesDropped?: number;
  nackCount?: number;
  pliCount?: number;
  firCount?: number;
  state?: string;
  nominated?: boolean;
  currentRoundTripTime?: number;
  availableOutgoingBitrate?: number;
  availableIncomingBitrate?: number;
  localCandidateId?: string;
  remoteCandidateId?: string;
}

export class StatsCollector {
  public static parseStatsReport(
    stats: Iterable<RawStatsReportEntry> | Map<string, RawStatsReportEntry> | Record<string, RawStatsReportEntry>,
    iceConnectionState = 'connected',
    connectionState = 'connected',
    signalingState = 'stable'
  ): RtpMetricsSnapshot {
    let totalBytesSent = 0;
    let totalBytesReceived = 0;

    let audioOutbound: RtpStreamMetrics | undefined;
    let videoOutbound: RtpStreamMetrics | undefined;
    let audioInbound: RtpStreamMetrics | undefined;
    let videoInbound: RtpStreamMetrics | undefined;
    let activeCandidatePair: CandidatePairMetrics | undefined;

    const entries: RawStatsReportEntry[] = [];
    if (stats instanceof Map) {
      for (const val of stats.values()) entries.push(val);
    } else if (Symbol.iterator in Object(stats)) {
      for (const val of stats as Iterable<RawStatsReportEntry>) entries.push(val);
    } else if (typeof stats === 'object' && stats !== null) {
      for (const val of Object.values(stats)) entries.push(val);
    }

    for (const report of entries) {
      if (!report || !report.type) continue;

      if (report.type === 'outbound-rtp') {
        const kind = (report.kind || report.mediaType || 'unknown') as 'audio' | 'video';
        const bytes = report.bytesSent ?? 0;
        totalBytesSent += bytes;

        const streamMetrics: RtpStreamMetrics = {
          ssrc: report.ssrc,
          kind,
          bytesSent: bytes,
          packetsSent: report.packetsSent,
          frameWidth: report.frameWidth,
          frameHeight: report.frameHeight,
          framesSent: report.framesSent,
          nackCount: report.nackCount,
          pliCount: report.pliCount,
          firCount: report.firCount
        };

        if (kind === 'audio') audioOutbound = streamMetrics;
        if (kind === 'video') videoOutbound = streamMetrics;
      } else if (report.type === 'inbound-rtp') {
        const kind = (report.kind || report.mediaType || 'unknown') as 'audio' | 'video';
        const bytes = report.bytesReceived ?? 0;
        totalBytesReceived += bytes;

        const streamMetrics: RtpStreamMetrics = {
          ssrc: report.ssrc,
          kind,
          bytesReceived: bytes,
          packetsReceived: report.packetsReceived,
          packetsLost: report.packetsLost,
          jitter: report.jitter,
          frameWidth: report.frameWidth,
          frameHeight: report.frameHeight,
          framesReceived: report.framesReceived,
          framesDecoded: report.framesDecoded,
          framesDropped: report.framesDropped,
          nackCount: report.nackCount,
          pliCount: report.pliCount,
          firCount: report.firCount
        };

        if (kind === 'audio') audioInbound = streamMetrics;
        if (kind === 'video') videoInbound = streamMetrics;
      } else if (report.type === 'candidate-pair' && (report.nominated || report.state === 'succeeded')) {
        activeCandidatePair = {
          state: report.state,
          nominated: report.nominated,
          currentRoundTripTimeMs: report.currentRoundTripTime ? report.currentRoundTripTime * 1000 : undefined,
          availableOutgoingBitrate: report.availableOutgoingBitrate,
          availableIncomingBitrate: report.availableIncomingBitrate
        };
      }
    }

    return {
      timestamp: new Date().toISOString(),
      iceConnectionState,
      connectionState,
      signalingState,
      activeCandidatePair,
      audioOutbound,
      audioInbound,
      videoOutbound,
      videoInbound,
      rawCounters: {
        totalBytesSent,
        totalBytesReceived
      }
    };
  }
}
