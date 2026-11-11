import { TokenFactory } from './token_factory.js';

export interface TwirpParticipantPermission {
  canSubscribe?: boolean;
  canPublish?: boolean;
  canPublishData?: boolean;
  hidden?: boolean;
  recorder?: boolean;
}

export interface TwirpTrackInfo {
  sid: string;
  type: number | string;
  name: string;
  muted: boolean;
  width?: number;
  height?: number;
  simulcast?: boolean;
}

export interface TwirpParticipantInfo {
  sid: string;
  identity: string;
  state: number | string;
  tracks: TwirpTrackInfo[];
  metadata: string;
  joinedAt: string;
  name: string;
  version: number;
  permission?: TwirpParticipantPermission;
}

export interface TwirpListParticipantsResponse {
  participants: TwirpParticipantInfo[];
}

export class TwirpRoomServiceClient {
  private baseUrl: string;
  private tokenFactory: TokenFactory;

  constructor(baseUrl = process.env.LIVEKIT_HTTP_URL || 'http://127.0.0.1:7880', tokenFactory = new TokenFactory()) {
    this.baseUrl = baseUrl.replace(/\/$/, '');
    this.tokenFactory = tokenFactory;
  }

  private async callTwirp<T>(method: string, body: Record<string, unknown>, roomId?: string): Promise<T> {
    const adminToken = await this.tokenFactory.createAdminToken(roomId);
    const url = `${this.baseUrl}/twirp/livekit.RoomService/${method}`;
    const res = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${adminToken}`
      },
      body: JSON.stringify(body)
    });

    if (!res.ok) {
      const errText = await res.text();
      throw new Error(`Twirp ${method} failed (${res.status}): ${errText}`);
    }

    const text = await res.text();
    return text ? (JSON.parse(text) as T) : ({} as T);
  }

  public async listParticipants(room: string): Promise<TwirpListParticipantsResponse> {
    return this.callTwirp<TwirpListParticipantsResponse>('ListParticipants', { room }, room);
  }

  public async updateParticipant(
    room: string,
    identity: string,
    options: { metadata?: string; permission?: TwirpParticipantPermission }
  ): Promise<TwirpParticipantInfo> {
    return this.callTwirp<TwirpParticipantInfo>(
      'UpdateParticipant',
      {
        room,
        identity,
        metadata: options.metadata,
        permission: options.permission
      },
      room
    );
  }

  public async mutePublishedTrack(
    room: string,
    identity: string,
    trackSid: string,
    muted: boolean
  ): Promise<Record<string, unknown>> {
    return this.callTwirp<Record<string, unknown>>(
      'MutePublishedTrack',
      {
        room,
        identity,
        track_sid: trackSid,
        muted
      },
      room
    );
  }

  public async sendData(
    room: string,
    data: Uint8Array | string,
    kind: 'RELIABLE' | 'LOSSY' = 'RELIABLE',
    options?: { destinationIdentities?: string[]; topic?: string }
  ): Promise<Record<string, unknown>> {
    const rawBytes = typeof data === 'string' ? Buffer.from(data, 'utf-8') : Buffer.from(data);
    const base64Data = rawBytes.toString('base64');
    return this.callTwirp<Record<string, unknown>>(
      'SendData',
      {
        room,
        data: base64Data,
        kind: kind === 'RELIABLE' ? 0 : 1,
        destination_identities: options?.destinationIdentities,
        topic: options?.topic
      },
      room
    );
  }

  public async removeParticipant(room: string, identity: string): Promise<Record<string, unknown>> {
    return this.callTwirp<Record<string, unknown>>('RemoveParticipant', { room, identity }, room);
  }
}
