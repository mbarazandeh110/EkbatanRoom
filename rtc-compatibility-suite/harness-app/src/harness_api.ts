import {
  Room,
  RoomEvent,
  VideoQuality,
  Track,
  RoomOptions,
  RoomConnectOptions,
  RemoteTrackPublication,
  RemoteParticipant,
  ConnectionState
} from 'livekit-client';

export interface HarnessState {
  status: 'IDLE' | 'CONNECTING' | 'CONNECTED' | 'DISCONNECTED' | 'ERROR';
  connectionState: string;
  roomName: string;
  participantIdentity: string;
  remoteParticipants: string[];
  subscribedTrackSids: string[];
  error?: string;
}

export class HarnessController {
  private room: Room | null = null;
  private state: HarnessState = {
    status: 'IDLE',
    connectionState: 'disconnected',
    roomName: '',
    participantIdentity: '',
    remoteParticipants: [],
    subscribedTrackSids: []
  };

  constructor() {
    this.updateDom();
  }

  public initRoom(options?: RoomOptions): Room {
    if (this.room) {
      this.room.disconnect();
    }

    this.room = new Room({
      adaptiveStream: false,
      dynacast: false,
      ...options
    });

    this.setupEventListeners(this.room);
    return this.room;
  }

  public getRoom(): Room | null {
    return this.room;
  }

  public getState(): HarnessState {
    if (this.room) {
      this.state.connectionState = this.room.state;
      this.state.roomName = this.room.name || '';
      this.state.participantIdentity = this.room.localParticipant?.identity || '';
      this.state.remoteParticipants = Array.from(this.room.remoteParticipants.keys());
    }
    return { ...this.state };
  }

  private setupEventListeners(room: Room): void {
    room.on(RoomEvent.ConnectionStateChanged, (state: ConnectionState) => {
      this.state.connectionState = state;
      if (state === ConnectionState.Connected) {
        this.state.status = 'CONNECTED';
      } else if (state === ConnectionState.Disconnected) {
        this.state.status = 'DISCONNECTED';
      } else if (state === ConnectionState.Connecting || state === ConnectionState.Reconnecting) {
        this.state.status = 'CONNECTING';
      }
      this.updateDom();
    });

    room.on(RoomEvent.Connected, () => {
      this.state.status = 'CONNECTED';
      this.state.roomName = room.name || '';
      this.state.participantIdentity = room.localParticipant.identity;
      this.updateDom();
    });

    room.on(RoomEvent.Disconnected, () => {
      this.state.status = 'DISCONNECTED';
      this.updateDom();
    });

    room.on(RoomEvent.ParticipantConnected, (p: RemoteParticipant) => {
      if (!this.state.remoteParticipants.includes(p.identity)) {
        this.state.remoteParticipants.push(p.identity);
      }
      this.updateDom();
    });

    room.on(RoomEvent.ParticipantDisconnected, (p: RemoteParticipant) => {
      this.state.remoteParticipants = this.state.remoteParticipants.filter(id => id !== p.identity);
      this.updateDom();
    });

    room.on(
      RoomEvent.TrackSubscribed,
      (track: Track, pub: RemoteTrackPublication, participant: RemoteParticipant) => {
        this.state.subscribedTrackSids.push(pub.trackSid);

        if (track.kind === Track.Kind.Video) {
          const videoEl = document.getElementById('remote-video') as HTMLVideoElement | null;
          if (videoEl) {
            track.attach(videoEl);
          }
        } else if (track.kind === Track.Kind.Audio) {
          const audioEl = document.getElementById('remote-audio') as HTMLAudioElement | null;
          if (audioEl) {
            track.attach(audioEl);
          }
        }
        this.updateDom();
      }
    );

    room.on(RoomEvent.TrackUnsubscribed, (track: Track, pub: RemoteTrackPublication) => {
      this.state.subscribedTrackSids = this.state.subscribedTrackSids.filter(sid => sid !== pub.trackSid);
      track.detach();
      this.updateDom();
    });
  }

  public async connect(url: string, token: string, options?: RoomConnectOptions): Promise<void> {
    if (!this.room) {
      this.initRoom();
    }
    this.state.status = 'CONNECTING';
    this.updateDom();

    try {
      await this.room!.connect(url, token, options);
      this.state.status = 'CONNECTED';
      this.state.roomName = this.room!.name || '';
      this.state.participantIdentity = this.room!.localParticipant.identity;
      this.updateDom();
    } catch (err: unknown) {
      this.state.status = 'ERROR';
      this.state.error = err instanceof Error ? err.message : String(err);
      this.updateDom();
      throw err;
    }
  }

  public async publishTracks(options: {
    audio?: boolean;
    video?: boolean;
    simulcast?: boolean;
  }): Promise<{ audioPublication?: string; videoPublication?: string }> {
    if (!this.room || this.room.state !== ConnectionState.Connected) {
      throw new Error('Cannot publish tracks when room is not connected');
    }

    const result: { audioPublication?: string; videoPublication?: string } = {};

    if (options.audio) {
      await this.room.localParticipant.setMicrophoneEnabled(true);
      const audioPub = Array.from(this.room.localParticipant.trackPublications.values()).find(
        p => p.kind === Track.Kind.Audio
      );
      if (audioPub) result.audioPublication = audioPub.trackSid;
    }

    if (options.video) {
      await this.room.localParticipant.setCameraEnabled(true, undefined, {
        simulcast: options.simulcast ?? false
      });
      const videoPub = Array.from(this.room.localParticipant.trackPublications.values()).find(
        p => p.kind === Track.Kind.Video
      );
      if (videoPub) {
        result.videoPublication = videoPub.trackSid;
        const videoEl = document.getElementById('local-video') as HTMLVideoElement | null;
        if (videoEl && videoPub.track) {
          videoPub.track.attach(videoEl);
        }
      }
    }

    return result;
  }

  public async unpublishTracks(): Promise<void> {
    if (!this.room) return;
    await this.room.localParticipant.setCameraEnabled(false);
    await this.room.localParticipant.setMicrophoneEnabled(false);
  }

  public async setMicrophoneEnabled(enabled: boolean): Promise<boolean> {
    if (!this.room) throw new Error('Room not initialized');
    await this.room.localParticipant.setMicrophoneEnabled(enabled);
    return this.room.localParticipant.isMicrophoneEnabled;
  }

  public async setCameraEnabled(enabled: boolean): Promise<boolean> {
    if (!this.room) throw new Error('Room not initialized');
    await this.room.localParticipant.setCameraEnabled(enabled);
    return this.room.localParticipant.isCameraEnabled;
  }

  public async setVideoQuality(quality: 'low' | 'medium' | 'high'): Promise<void> {
    if (!this.room) throw new Error('Room not initialized');

    let lkQuality: VideoQuality = VideoQuality.HIGH;
    if (quality === 'low') lkQuality = VideoQuality.LOW;
    else if (quality === 'medium') lkQuality = VideoQuality.MEDIUM;

    for (const pub of this.room.remoteParticipants.values()) {
      for (const trackPub of pub.trackPublications.values()) {
        if (trackPub.kind === Track.Kind.Video && trackPub instanceof RemoteTrackPublication) {
          trackPub.setVideoQuality(lkQuality);
        }
      }
    }
  }

  public async restartIce(): Promise<void> {
    if (!this.room || !this.room.engine) {
      throw new Error('Room or engine not available for ICE restart');
    }
    // Room in official livekit-client exposes restartIce or engine.restartIce
    const engine = (this.room as unknown as { engine: { restartIce: () => Promise<void> } }).engine;
    if (engine && typeof engine.restartIce === 'function') {
      await engine.restartIce();
    } else {
      throw new Error('restartIce not supported by engine');
    }
  }

  public async getPeerConnectionStats(): Promise<Record<string, unknown>[]> {
    if (!this.room || !this.room.engine) {
      return [];
    }

    const engine = (this.room as unknown as {
      engine: {
        pcManager?: {
          publisher?: RTCPeerConnection;
          subscriber?: RTCPeerConnection;
        };
      };
    }).engine;

    const results: Record<string, unknown>[] = [];
    const pcs = [
      engine.pcManager?.publisher,
      engine.pcManager?.subscriber
    ].filter(Boolean) as RTCPeerConnection[];

    for (const pc of pcs) {
      try {
        const stats = await pc.getStats();
        stats.forEach((report: unknown) => {
          results.push(report as Record<string, unknown>);
        });
      } catch (e) {
        console.warn('Failed to getStats from pc', e);
      }
    }

    return results;
  }

  public getRenderedVideoStats(selector = '#remote-video'): {
    videoWidth: number;
    videoHeight: number;
    readyState: number;
    currentTime: number;
  } {
    const el = document.querySelector(selector) as HTMLVideoElement | null;
    if (!el) {
      return { videoWidth: 0, videoHeight: 0, readyState: 0, currentTime: 0 };
    }
    return {
      videoWidth: el.videoWidth,
      videoHeight: el.videoHeight,
      readyState: el.readyState,
      currentTime: el.currentTime
    };
  }

  public async disconnect(): Promise<void> {
    if (this.room) {
      await this.room.disconnect();
      this.state.status = 'DISCONNECTED';
      this.updateDom();
    }
  }

  private updateDom(): void {
    const statusEl = document.getElementById('harness-status');
    const connStateEl = document.getElementById('connection-state');
    const roomEl = document.getElementById('room-name');
    const partEl = document.getElementById('participant-identity');
    const countEl = document.getElementById('remote-participants-count');

    if (statusEl) statusEl.textContent = this.state.status;
    if (connStateEl) connStateEl.textContent = this.state.connectionState;
    if (roomEl) roomEl.textContent = this.state.roomName || '-';
    if (partEl) partEl.textContent = this.state.participantIdentity || '-';
    if (countEl) countEl.textContent = String(this.state.remoteParticipants.length);
  }
}
