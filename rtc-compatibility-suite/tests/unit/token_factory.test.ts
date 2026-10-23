import { describe, it, expect } from 'vitest';
import { jwtVerify } from 'jose';
import { TokenFactory } from '../../src/controllers/token_factory.js';

describe('TokenFactory', () => {
  const testSecret = 'test-secret-key-12345';
  const factory = new TokenFactory({
    apiKey: 'test-api-key',
    apiSecret: testSecret,
    defaultTtlSeconds: 300
  });

  it('creates a valid signed JWT with LiveKit video claims', async () => {
    const token = await factory.createToken('room-alpha', 'participant-1', {
      name: 'Alice',
      metadata: 'role=publisher'
    });

    expect(token).toBeDefined();
    expect(typeof token).toBe('string');
    expect(token.split('.')).toHaveLength(3);

    // Verify token using secret
    const secretKey = new TextEncoder().encode(testSecret);
    const { payload } = await jwtVerify(token, secretKey);

    expect(payload.sub).toBe('participant-1');
    expect(payload.iss).toBe('test-api-key');
    expect(payload.name).toBe('Alice');
    expect(payload.metadata).toBe('role=publisher');

    const videoClaim = payload.video as Record<string, unknown>;
    expect(videoClaim).toBeDefined();
    expect(videoClaim.room).toBe('room-alpha');
    expect(videoClaim.roomJoin).toBe(true);
    expect(videoClaim.canPublish).toBe(true);
    expect(videoClaim.canSubscribe).toBe(true);
  });
});
