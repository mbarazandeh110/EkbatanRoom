import { SignJWT } from 'jose';
import type { ITokenFactory, TokenOptions } from './interfaces.js';

export interface TokenFactoryConfig {
  apiKey?: string;
  apiSecret?: string;
  defaultTtlSeconds?: number;
}

export class TokenFactory implements ITokenFactory {
  private apiKey: string;
  private apiSecret: Uint8Array;
  private defaultTtlSeconds: number;

  constructor(config?: TokenFactoryConfig) {
    this.apiKey = config?.apiKey || process.env.LIVEKIT_API_KEY || 'devkey';
    const secretStr = config?.apiSecret || process.env.LIVEKIT_API_SECRET || 'secret';
    this.apiSecret = new TextEncoder().encode(secretStr);
    this.defaultTtlSeconds = config?.defaultTtlSeconds || 600; // 10 minutes default
  }

  public async createToken(roomId: string, identity: string, options?: TokenOptions): Promise<string> {
    const now = Math.floor(Date.now() / 1000);
    const ttl = options?.ttlSeconds ?? this.defaultTtlSeconds;

    const payload = {
      sub: identity,
      iss: this.apiKey,
      nbf: now - 5,
      exp: now + ttl,
      name: options?.name || identity,
      metadata: options?.metadata,
      video: {
        room: roomId,
        roomJoin: true,
        canPublish: true,
        canSubscribe: true,
        canPublishData: true
      }
    };

    const token = await new SignJWT(payload)
      .setProtectedHeader({ alg: 'HS256', typ: 'JWT' })
      .setIssuedAt(now)
      .setExpirationTime(now + ttl)
      .sign(this.apiSecret);

    return token;
  }
}
