/**
 * Standard WebRTC QoE and Network Metrics Snapshot extracted from RTCPeerConnection.getStats().
 */
export interface RtpStreamMetrics {
  ssrc?: number;
  kind: 'audio' | 'video';
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
}

export interface CandidatePairMetrics {
  state?: string;
  nominated?: boolean;
  currentRoundTripTimeMs?: number;
  availableOutgoingBitrate?: number;
  availableIncomingBitrate?: number;
  localCandidateType?: string;
  remoteCandidateType?: string;
  localCandidateAddress?: string;
  remoteCandidateAddress?: string;
}

export interface RtpMetricsSnapshot {
  timestamp: string;
  iceConnectionState: string;
  connectionState: string;
  signalingState: string;
  activeCandidatePair?: CandidatePairMetrics;
  audioOutbound?: RtpStreamMetrics;
  audioInbound?: RtpStreamMetrics;
  videoOutbound?: RtpStreamMetrics;
  videoInbound?: RtpStreamMetrics;
  rawCounters: {
    totalBytesSent: number;
    totalBytesReceived: number;
  };
}
