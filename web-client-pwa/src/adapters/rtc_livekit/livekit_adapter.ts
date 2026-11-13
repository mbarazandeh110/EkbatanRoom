import {
  Room,
  RoomEvent,
  Track as LkTrack,
  VideoQuality as LkVideoQuality,
  ConnectionState as LkConnectionState,
  Participant as LkParticipant,
  RemoteParticipant as LkRemoteParticipant,
  RemoteTrackPublication,
  TrackPublication as LkTrackPublication,
  RemoteTrack as LkRemoteTrack,
  ConnectionQuality as LkConnectionQuality,
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

/**
 * Conforms SDP Answer directions to RFC 3264 requirements based on the Local Offer.
 * Specifically prevents "Incompatible send direction" exceptions in Chromium when
 * an m-section was offered as inactive (e.g. unpublished screen share) but the remote
 * answer responds with recvonly or sendrecv.
 */
export function conformAnswerSdp(answerSdp: string, offerSdp?: string): string {
  if (!offerSdp || !answerSdp) return answerSdp;

  const splitSections = (sdp: string) => {
    const lines = sdp.split(/\r?\n/);
    const sections: string[][] = [];
    let current: string[] = [];
    for (const line of lines) {
      if (line.startsWith("m=") && sections.length > 0) {
        sections.push(current);
        current = [line];
      } else if (line.startsWith("m=") && sections.length === 0) {
        sections.push(current);
        current = [line];
      } else {
        current.push(line);
      }
    }
    if (current.length > 0) {
      sections.push(current);
    }
    return sections;
  };

  const getMidAndDir = (lines: string[]) => {
    let mid: string | null = null;
    let dir = "sendrecv";
    let hasExplicitDir = false;
    for (const l of lines) {
      const trimmed = l.trim();
      if (trimmed.startsWith("a=mid:")) {
        mid = trimmed.slice(6).trim();
      } else if (
        trimmed === "a=sendrecv" ||
        trimmed === "a=sendonly" ||
        trimmed === "a=recvonly" ||
        trimmed === "a=inactive"
      ) {
        dir = trimmed.slice(2).trim();
        hasExplicitDir = true;
      }
    }
    return { mid, dir, hasExplicitDir };
  };

  const offerSections = splitSections(offerSdp);
  const offerMap = new Map<string, string>();
  const offerByIndex: string[] = [];

  for (let i = 1; i < offerSections.length; i++) {
    const info = getMidAndDir(offerSections[i]);
    if (info.mid) {
      offerMap.set(info.mid, info.dir);
    }
    offerByIndex.push(info.dir);
  }

  const answerSections = splitSections(answerSdp);
  let mediaIndex = 0;
  for (let i = 1; i < answerSections.length; i++) {
    const sec = answerSections[i];
    const info = getMidAndDir(sec);
    const offerDir =
      (info.mid ? offerMap.get(info.mid) : null) ||
      offerByIndex[mediaIndex] ||
      "sendrecv";
    mediaIndex++;

    let validAnswerDir = info.dir;
    if (offerDir === "inactive") {
      validAnswerDir = "inactive";
    } else if (offerDir === "recvonly" && (info.dir === "recvonly" || info.dir === "sendrecv")) {
      validAnswerDir = "inactive";
    } else if (offerDir === "sendonly" && (info.dir === "sendonly" || info.dir === "sendrecv")) {
      validAnswerDir = "inactive";
    }

    if (validAnswerDir !== info.dir || (!info.hasExplicitDir && offerDir === "inactive")) {
      let replaced = false;
      for (let j = 0; j < sec.length; j++) {
        const trimmed = sec[j].trim();
        if (
          trimmed === "a=sendrecv" ||
          trimmed === "a=sendonly" ||
          trimmed === "a=recvonly" ||
          trimmed === "a=inactive"
        ) {
          sec[j] = `a=${validAnswerDir}`;
          replaced = true;
          break;
        }
      }
      if (!replaced) {
        sec.push(`a=${validAnswerDir}`);
      }
    }
  }

  return answerSections.map((s) => s.join("\r\n")).join("\r\n");
}

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
  window.RTCPeerConnection.prototype.setRemoteDescription = async function (desc: RTCSessionDescriptionInit) {
    if (desc && desc.sdp) {
      let rewrittenSdp = desc.sdp;
      if (currentTargetLanIp && rewrittenSdp.includes("127.0.0.1")) {
        rewrittenSdp = rewrittenSdp.replaceAll("127.0.0.1", currentTargetLanIp);
      }
      if (desc.type === "answer" && this.localDescription && this.localDescription.sdp) {
        rewrittenSdp = conformAnswerSdp(rewrittenSdp, this.localDescription.sdp);
      }
      return Reflect.apply(origSetRemote, this, [{ type: desc.type, sdp: rewrittenSdp }]);
    }
    return Reflect.apply(origSetRemote, this, [desc]);
  };

  const origAddIce = window.RTCPeerConnection.prototype.addIceCandidate;
  window.RTCPeerConnection.prototype.addIceCandidate = async function (cand?: RTCIceCandidateInit | RTCIceCandidate) {
    if (
      currentTargetLanIp &&
      cand &&
      typeof cand === "object" &&
      "candidate" in cand &&
      typeof cand.candidate === "string" &&
      cand.candidate.includes("127.0.0.1")
    ) {
      const rewrittenCand = cand.candidate.replaceAll("127.0.0.1", currentTargetLanIp);
      return Reflect.apply(origAddIce, this, [{ ...cand, candidate: rewrittenCand }]);
    }
    return Reflect.apply(origAddIce, this, [cand]);
  };
}

export class LiveKitEngineAdapter implements RtcEnginePort {
  private room: Room | null = null;
  private attachedAudioElements: Set<HTMLMediaElement> = new Set();
  private eventListeners: {
    [K in keyof RtcEngineEvents]?: Array<RtcEngineEvents[K]>;
  } = {};

  constructor(
    private telemetry?: TelemetryPort,
    private roomFactory: () => Room = () =>
      new Room({
        adaptiveStream: false,
        dynacast: true,
        singlePeerConnection: false,
      })
  ) {}

  public on<E extends keyof RtcEngineEvents>(
    event: E,
    handler: RtcEngineEvents[E]
  ): void {
    const list = this.eventListeners[event];
    if (!list) {
      (this.eventListeners as Record<keyof RtcEngineEvents, unknown>)[event] = [handler];
    } else {
      (list as Array<RtcEngineEvents[E]>).push(handler);
    }
  }

  public off<E extends keyof RtcEngineEvents>(
    event: E,
    handler: RtcEngineEvents[E]
  ): void {
    const list = this.eventListeners[event];
    if (list) {
      (this.eventListeners as Record<keyof RtcEngineEvents, unknown>)[event] =
        list.filter((h) => h !== handler);
    }
  }

  public emit<E extends keyof RtcEngineEvents>(
    event: E,
    ...args: Parameters<RtcEngineEvents[E]>
  ): void {
    const handlers = this.eventListeners[event];
    if (handlers) {
      const typedHandlers = handlers as Array<(...a: Parameters<RtcEngineEvents[E]>) => void>;
      typedHandlers.forEach((h) => h(...args));
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

    // Hydrate existing remote participants already present upon room connection
    for (const participant of this.room.remoteParticipants.values()) {
      this.emit("participantJoined", this.mapParticipant(participant));

      // Inspect any tracks that might already be subscribed upon join
      for (const pub of participant.trackPublications.values()) {
        if (pub.track && pub.isSubscribed) {
          if (pub.track.kind === LkTrack.Kind.Audio) {
            try {
              const el = pub.track.attach();
              if (el) {
                this.attachedAudioElements.add(el);
                if (typeof document !== "undefined" && !el.parentElement) {
                  el.style.display = "none";
                  document.body.appendChild(el);
                }
              }
            } catch (err) {
              this.telemetry?.log("warn", "audio_track_attach_error", { error: String(err) });
            }
          }

          const descriptor: TrackDescriptor = {
            id: pub.trackSid || (pub.track.sid as string) || pub.source,
            kind: pub.track.kind as TrackKind,
            source: this.mapTrackSource(pub.source),
            participantId: participant.sid || participant.identity,
            isMuted: pub.isMuted,
            isEnabled: pub.isEnabled,
            mediaStreamTrack: pub.track.mediaStreamTrack,
          };
          this.emit("trackSubscribed", descriptor, this.mapParticipant(participant));
        }
      }
    }
  }

  public async disconnect(): Promise<void> {
    if (this.room) {
      for (const el of this.attachedAudioElements) {
        if (el && typeof el.remove === "function") {
          el.remove();
        }
      }
      this.attachedAudioElements.clear();
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

    const isScreenShare = options.source === "screen";
    const lkSource = this.mapDomainSourceToLk(options.source);
    const publication = await this.room.localParticipant.publishTrack(track, {
      simulcast: isScreenShare ? false : (options.simulcast ?? true),
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
      if (
        pub.trackSid === trackId ||
        pub.track?.sid === trackId ||
        (trackId === "screen" && pub.source === LkTrack.Source.ScreenShare) ||
        (trackId === "camera" && pub.source === LkTrack.Source.Camera) ||
        (trackId === "microphone" && pub.source === LkTrack.Source.Microphone)
      ) {
        if (pub.track) {
          await this.room.localParticipant.unpublishTrack(pub.track, true);
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
      let pub: RemoteTrackPublication | undefined;
      if (participant.trackPublications?.values) {
        pub = Array.from(participant.trackPublications.values()).find(
          (p) => p.trackSid === trackId || p.track?.sid === trackId
        ) as RemoteTrackPublication | undefined;
      }
      if (!pub && typeof participant.getTrackPublication === "function") {
        const lookup = (participant.getTrackPublication as unknown as (id: string) => RemoteTrackPublication | undefined)(trackId);
        if (lookup) pub = lookup;
      }
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
      let pub: RemoteTrackPublication | undefined;
      if (participant.trackPublications?.values) {
        pub = Array.from(participant.trackPublications.values()).find(
          (p) => p.trackSid === trackId || p.track?.sid === trackId
        ) as RemoteTrackPublication | undefined;
      }
      if (!pub && typeof participant.getTrackPublication === "function") {
        const lookup = (participant.getTrackPublication as unknown as (id: string) => RemoteTrackPublication | undefined)(trackId);
        if (lookup) pub = lookup;
      }
      if (pub instanceof RemoteTrackPublication) {
        pub.setVideoQuality(lkQuality);
        return;
      }
    }
  }

  public async sendData(
    data: Uint8Array | string,
    options?: { topic?: string; reliable?: boolean; destinationIdentities?: string[] }
  ): Promise<void> {
    if (!this.room || !this.room.localParticipant) {
      throw new Error("Cannot send data: Room not connected");
    }
    const payload = typeof data === "string" ? new TextEncoder().encode(data) : data;
    await this.room.localParticipant.publishData(payload, {
      reliable: options?.reliable ?? true,
      destinationIdentities: options?.destinationIdentities,
      topic: options?.topic,
    });
  }

  public async restartIce(): Promise<void> {
    if (!this.room) return;
    this.telemetry?.log("info", "livekit_ice_restart_requested");
    const roomWithReconnect = this.room as unknown as { reconnect?: () => Promise<void> };
    if (typeof roomWithReconnect.reconnect === "function") {
      await roomWithReconnect.reconnect();
    }
  }

  private bindRoomEvents(room: Room): void {
    room.on(RoomEvent.ConnectionStateChanged, (state: LkConnectionState) => {
      this.emit("connectionStatusChanged", this.mapConnectionState(state));
    });

    room.on(RoomEvent.Reconnecting, () => {
      this.emit("connectionStatusChanged", "reconnecting");
      this.emit("reconnecting");
    });

    room.on(RoomEvent.Reconnected, () => {
      this.emit("connectionStatusChanged", "connected");
      this.emit("reconnected");
    });

    room.on(
      RoomEvent.DataReceived,
      (payload: Uint8Array, participant?: LkRemoteParticipant, _kind?: unknown, topic?: string) => {
        const pModel = participant ? this.mapParticipant(participant) : undefined;
        this.emit("dataReceived", payload, pModel, topic);
      }
    );

    room.on(RoomEvent.ParticipantConnected, (p: LkParticipant) => {
      this.emit("participantJoined", this.mapParticipant(p));
    });

    room.on(RoomEvent.ParticipantDisconnected, (p: LkParticipant) => {
      this.emit("participantLeft", p.sid || p.identity);
    });

    room.on(
      RoomEvent.TrackSubscribed,
      (track: LkRemoteTrack, pub: RemoteTrackPublication, participant: LkRemoteParticipant) => {
        if (track && ((track.kind as string) === "audio" || (track.kind as string) === LkTrack.Kind.Audio)) {
          try {
            const el = track.attach();
            if (el) {
              this.attachedAudioElements.add(el);
              if (typeof document !== "undefined" && !el.parentElement) {
                el.style.display = "none";
                document.body.appendChild(el);
              }
            }
          } catch (err) {
            this.telemetry?.log("warn", "audio_track_attach_error", { error: String(err) });
          }
        }

        const descriptor: TrackDescriptor = {
          id: pub.trackSid || track.sid || "",
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
      (track: LkRemoteTrack, pub: RemoteTrackPublication, participant: LkRemoteParticipant) => {
        if (track && ((track.kind as string) === "audio" || (track.kind as string) === LkTrack.Kind.Audio)) {
          try {
            const detached = track.detach();
            const elements = Array.isArray(detached) ? detached : detached ? [detached] : [];
            for (const el of elements) {
              this.attachedAudioElements.delete(el);
              if (el && typeof el.remove === "function") {
                el.remove();
              }
            }
          } catch {
            // ignore
          }
        }

        this.emit(
          "trackUnsubscribed",
          pub.trackSid || track.sid || "",
          participant.sid || participant.identity
        );
      }
    );

    room.on(
      RoomEvent.TrackMuted,
      (pub: LkTrackPublication, participant: LkParticipant) => {
        this.emit(
          "trackMutedChanged",
          pub.trackSid || pub.track?.sid || "",
          participant.sid || participant.identity,
          true
        );
      }
    );

    room.on(
      RoomEvent.TrackUnmuted,
      (pub: LkTrackPublication, participant: LkParticipant) => {
        this.emit(
          "trackMutedChanged",
          pub.trackSid || pub.track?.sid || "",
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
      (quality: LkConnectionQuality, participant: LkParticipant) => {
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
