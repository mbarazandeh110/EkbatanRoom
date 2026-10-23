export interface BrowserTestHarness {
  initRoom(options?: unknown): unknown;
  getRoom(): unknown;
  getState(): {
    status: 'IDLE' | 'CONNECTING' | 'CONNECTED' | 'DISCONNECTED' | 'ERROR';
    connectionState: string;
    roomName: string;
    participantIdentity: string;
    remoteParticipants: string[];
    subscribedTrackSids: string[];
    error?: string;
  };
  connect(url: string, token: string, options?: unknown): Promise<void>;
  publishTracks(options: { audio?: boolean; video?: boolean; simulcast?: boolean }): Promise<{
    audioPublication?: string;
    videoPublication?: string;
  }>;
  unpublishTracks(): Promise<void>;
  setMicrophoneEnabled(enabled: boolean): Promise<boolean>;
  setCameraEnabled(enabled: boolean): Promise<boolean>;
  setVideoQuality(quality: 'low' | 'medium' | 'high'): Promise<void>;
  restartIce(): Promise<void>;
  getPeerConnectionStats(): Promise<Record<string, unknown>[]>;
  getRenderedVideoStats(selector?: string): {
    videoWidth: number;
    videoHeight: number;
    readyState: number;
    currentTime: number;
  };
  disconnect(): Promise<void>;
}

declare global {
  interface Window {
    testHarness: BrowserTestHarness;
  }
}
