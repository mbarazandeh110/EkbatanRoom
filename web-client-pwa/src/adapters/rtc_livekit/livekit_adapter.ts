import {
  Room,
  RoomEvent,
  Track as LkTrack,
  VideoQuality as LkVideoQuality,
  ConnectionState as LkConnectionState,
  Participant as LkParticipant,
  RemoteTrackPublication,
  LocalTrackPublication,
} from "livekit-client";
import {
  RtcEnginePort,
  ConnectOptions,
  PublishTrackOptions,
  RtcEngineEvents,
  ActiveSpeakerInfo,
} from "../../ports/rtc_engine_port";
import {
  ConnectionStatus,
  ParticipantModel,
  TrackDescriptor,
  TrackSource,
  TrackKind,
  VideoQualityLayer,
  ConnectionQualityLevel,
} from "../../ports/rtc_engine_port";
import { TelemetryPort } from "../../ports/telemetry_port";

let isPeerConnectionPatched = false;
let currentTargetLanIp: string | null = null;

function ensurePeerConnectionPatched(lanIp?: string) {
  if (lanIp) {
    currentTargetLanIp = lanIp;
  }
  if (
    typeof window === "undefined" ||
    !window.RTCPeerConnection ||
    isPeerConnectionPatched
  ) {
    return;
  }
  isPeerConnectionPatched = true;

  const origSetRemote = window.RTCPeerConnection.prototype.setRemoteDescription;
  window.RTCPeerConnection.prototype.setRemoteDescription = async function (desc) {
    if (currentTargetLanIp && desc && desc.sdp && desc.sdp.includes("127.0.0.1")) {
      const rewrittenSdp = desc.sdp.replaceAll("127.0.0.1", currentTargetLanIp);
      return (origSetRemote as any).call(this, { type: desc.type, sdp: rewrittenSdp });
    }
    return (origSetRemote as any).call(this, desc);
  };

  const origAddIce = window.RTCPeerConnection.prototype.addIceCandidate;
  window.RTCPeerConnection.prototype.addIceCandidate = async function (cand) {
    if (
      currentTargetLanIp &&
      cand &&
      typeof cand === "object" &&
      "candidate" in cand &&
      typeof cand.candidate === "string" &&
      cand.candidate.includes("127.0.0.1")
    ) {
      const rewrittenCand = cand.candidate.replaceAll("127.0.0.1", currentTargetLanIp);
      return (origAddIce as any).call(this, { ...cand, candidate: rewrittenCand });
    }
    return (origAddIce as any).call(this, cand);
  };
}

export class LiveKitEngineAdapter implements RtcEnginePort {
  private room: Room | null = null;
  private eventListeners: {
    [K in keyof RtcEngineEvents]?: Array<RtcEngineEvents[K]>;
  } = {};

  constructor(
    private telemetry?: TelemetryPort,
    private roomFactory: () => Room = () =>
      new Room({
        adaptiveStream: true,
        dynacast: true,
        singlePeerConnection: false,
      })
  ) {}

  public on<E extends keyof RtcEngineEvents>(
    event: E,
    handler: RtcEngineEvents[E]
  ): void {
    if (!this.eventListeners[event]) {
      this.eventListeners[event] = [];
    }
    this.eventListeners[event]!.push(handler);
  }

  public off<E extends keyof RtcEngineEvents>(
    event: E,
    handler: RtcEngineEvents[E]
  ): void {
    const list = this.eventListeners[event];
    if (list) {
      this.eventListeners[event] = list.filter((h) => h !== handler) as any;
    }
  }

  public emit<E extends keyof RtcEngineEvents>(
    event: E,
    ...args: Parameters<RtcEngineEvents[E]>
  ): void {
    const handlers = this.eventListeners[event];
    if (handlers) {
      handlers.forEach((h: any) => h(...args));
    }
  }

  private mapConnectionState(state: LkConnectionState): ConnectionStatus {
    switch (state) {
      case LkConnectionState.Connected:
        return "connected";
      case LkConnectionState.Connecting:
        return "connecting";
      case LkConnectionState.Reconnecting:
        return "reconnecting";
      case LkConnectionState.Disconnected:
      default:
        return "disconnected";
    }
  }

  private mapTrackSource(source: LkTrack.Source): TrackSource {
    switch (source) {
      case LkTrack.Source.Camera:
        return "camera";
      case LkTrack.Source.Microphone:
        return "microphone";
      case LkTrack.Source.ScreenShare:
      case LkTrack.Source.ScreenShareAudio:
        return "screen";
      default:
        return "camera";
    }
  }

  private mapDomainSourceToLk(source: TrackSource): LkTrack.Source {
    switch (source) {
      case "camera":
        return LkTrack.Source.Camera;
      case "microphone":
        return LkTrack.Source.Microphone;
      case "screen":
        return LkTrack.Source.ScreenShare;
    }
  }

  private mapVideoQuality(quality: VideoQualityLayer): LkVideoQuality {
    switch (quality) {
      case "low":
        return LkVideoQuality.LOW;
      case "medium":
        return LkVideoQuality.MEDIUM;
      case "high":
        return LkVideoQuality.HIGH;
    }
  }

  private mapParticipant(p: LkParticipant): ParticipantModel {
    let quality: ConnectionQualityLevel = "good";
    const lkQuality = p.connectionQuality?.toString().toLowerCase();
    if (lkQuality === "excellent") quality = "excellent";
    else if (lkQuality === "poor") quality = "poor";
    else if (lkQuality === "lost") quality = "lost";

    return {
      id: p.sid || p.identity,
      identity: p.identity,
      name: p.name || p.identity,
      isLocal: p === this.room?.localParticipant,
      role: "speaker",
      isSpeaking: p.isSpeaking,
      audioLevel: p.audioLevel,
      connectionQuality: quality,
      isMicMuted: !p.isMicrophoneEnabled,
      isCameraMuted: !p.isCameraEnabled,
      isScreenSharing: p.isScreenShareEnabled,
      joinedAt: p.joinedAt?.getTime() ?? Date.now(),
    };
  }

  public async connect(options: ConnectOptions): Promise<void> {
    if (!this.room) {
      this.room = this.roomFactory();
      this.bindRoomEvents(this.room);
    }

    const targetLanIp =
      options.lanIp ||
      (typeof process !== "undefined"
        ? process.env.NEXT_PUBLIC_SFU_ANNOUNCED_IP
        : undefined) ||
      (typeof window !== "undefined" &&
      window.location.hostname !== "localhost" &&
      /^(\d{1,3}\.){3}\d{1,3}$/.test(window.location.hostname)
        ? window.location.hostname
        : undefined);

    if (targetLanIp) {
      ensurePeerConnectionPatched(targetLanIp);
    }

    this.telemetry?.log("info", "livekit_connecting", {
      url: options.url,
      roomName: options.roomName,
    });

    const defaultIceServers: RTCIceServer[] = [
      { urls: "stun:stun.l.google.com:19302" },
    ];
    let customIceServers: RTCIceServer[] | undefined;
    if (typeof process !== "undefined" && process.env.NEXT_PUBLIC_RTC_ICE_SERVERS) {
      try {
        customIceServers = JSON.parse(process.env.NEXT_PUBLIC_RTC_ICE_SERVERS);
      } catch {
        // ignore parse error
      }
    }

    await this.room.connect(options.url, options.token, {
      autoSubscribe: true,
      rtcConfig: {
        iceServers: customIceServers || defaultIceServers,
      },
    });
  }

  public async disconnect(): Promise<void> {
    if (this.room) {
      await this.room.disconnect();
      this.room = null;
    }
  }

  public getConnectionStatus(): ConnectionStatus {
    if (!this.room) return "disconnected";
    return this.mapConnectionState(this.room.state);
  }

  public getRoomId(): string {
    return this.room?.name || "";
  }

  public getLocalParticipant(): ParticipantModel | null {
    if (!this.room?.localParticipant) return null;
    return this.mapParticipant(this.room.localParticipant);
  }

  public async publishTrack(
    track: MediaStreamTrack,
    options: PublishTrackOptions
  ): Promise<TrackDescriptor> {
    if (!this.room?.localParticipant) {
      throw new Error("Cannot publish track: not connected to room");
    }

    const lkSource = this.mapDomainSourceToLk(options.source);
    const publication = await this.room.localParticipant.publishTrack(track, {
      simulcast: options.simulcast ?? true,
      source: lkSource,
    });

    return {
      id: publication.trackSid || publication.track?.sid || `local-${options.source}`,
      kind: track.kind as TrackKind,
      source: options.source,
      participantId: this.room.localParticipant.sid || this.room.localParticipant.identity,
      isMuted: publication.isMuted,
      isEnabled: true,
      mediaStreamTrack: track,
    };
  }

  public async unpublishTrack(trackId: string): Promise<void> {
    if (!this.room?.localParticipant) return;

    for (const pub of this.room.localParticipant.trackPublications.values()) {
      if (pub.trackSid === trackId || pub.track?.sid === trackId) {
        if (pub.track) {
          await this.room.localParticipant.unpublishTrack(pub.track);
        }
        return;
      }
    }
  }

  public async setTrackMuted(source: TrackSource, muted: boolean): Promise<void> {
    if (!this.room?.localParticipant) return;

    const lkSource = this.mapDomainSourceToLk(source);
    for (const pub of this.room.localParticipant.trackPublications.values()) {
      if (pub.source === lkSource) {
        if (muted) {
          await pub.mute();
        } else {
          await pub.unmute();
        }
      }
    }
  }

  public isTrackMuted(source: TrackSource): boolean {
    if (!this.room?.localParticipant) return true;
    const lkSource = this.mapDomainSourceToLk(source);
    for (const pub of this.room.localParticipant.trackPublications.values()) {
      if (pub.source === lkSource) {
        return pub.isMuted;
      }
    }
    return false;
  }

  public async setTrackEnabled(trackId: string, enabled: boolean): Promise<void> {
    if (!this.room) return;

    for (const participant of this.room.remoteParticipants.values()) {
      const pub = participant.getTrackPublication(trackId as any);
      if (pub instanceof RemoteTrackPublication) {
        pub.setEnabled(enabled);
        return;
      }
    }
  }

  public async setVideoQuality(
    trackId: string,
    quality: VideoQualityLayer
  ): Promise<void> {
    if (!this.room) return;

    const lkQuality = this.mapVideoQuality(quality);
    for (const participant of this.room.remoteParticipants.values()) {
      const pub = participant.getTrackPublication(trackId as any);
      if (pub instanceof RemoteTrackPublication) {
        pub.setVideoQuality(lkQuality);
        return;
      }
    }
  }

  private bindRoomEvents(room: Room): void {
    room.on(RoomEvent.ConnectionStateChanged, (state: LkConnectionState) => {
      this.emit("connectionStatusChanged", this.mapConnectionState(state));
    });

    room.on(RoomEvent.ParticipantConnected, (p: LkParticipant) => {
      this.emit("participantJoined", this.mapParticipant(p));
    });

    room.on(RoomEvent.ParticipantDisconnected, (p: LkParticipant) => {
      this.emit("participantLeft", p.sid || p.identity);
    });

    room.on(
      RoomEvent.TrackSubscribed,
      (track: any, pub: RemoteTrackPublication, participant: LkParticipant) => {
        const descriptor: TrackDescriptor = {
          id: pub.trackSid || track.sid,
          kind: track.kind as TrackKind,
          source: this.mapTrackSource(pub.source),
          participantId: participant.sid || participant.identity,
          isMuted: pub.isMuted,
          isEnabled: pub.isEnabled,
          mediaStreamTrack: track.mediaStreamTrack,
        };
        this.emit("trackSubscribed", descriptor, this.mapParticipant(participant));
      }
    );

    room.on(
      RoomEvent.TrackUnsubscribed,
      (track: any, pub: RemoteTrackPublication, participant: LkParticipant) => {
        this.emit(
          "trackUnsubscribed",
          pub.trackSid || track.sid,
          participant.sid || participant.identity
        );
      }
    );

    room.on(
      RoomEvent.TrackMuted,
      (pub: any, participant: LkParticipant) => {
        this.emit(
          "trackMutedChanged",
          pub.trackSid || pub.track?.sid,
          participant.sid || participant.identity,
          true
        );
      }
    );

    room.on(
      RoomEvent.TrackUnmuted,
      (pub: any, participant: LkParticipant) => {
        this.emit(
          "trackMutedChanged",
          pub.trackSid || pub.track?.sid,
          participant.sid || participant.identity,
          false
        );
      }
    );

    room.on(RoomEvent.ActiveSpeakersChanged, (speakers: LkParticipant[]) => {
      const active: ActiveSpeakerInfo[] = speakers.map((s) => ({
        participantId: s.sid || s.identity,
        audioLevel: s.audioLevel,
      }));
      this.emit("activeSpeakersChanged", active);
    });

    room.on(
      RoomEvent.ConnectionQualityChanged,
      (quality: any, participant: LkParticipant) => {
        let qLevel: ConnectionQualityLevel = "good";
        const qStr = quality?.toString().toLowerCase();
        if (qStr === "excellent") qLevel = "excellent";
        else if (qStr === "poor") qLevel = "poor";
        else if (qStr === "lost") qLevel = "lost";

        this.emit(
          "connectionQualityChanged",
          participant.sid || participant.identity,
          qLevel
        );
      }
    );
  }
}
