import { describe, it, expect } from 'vitest';
import { redactSensitiveData, sanitizePayload } from '../../src/reporters/redaction.js';

describe('Security Redaction', () => {
  it('redacts raw JWT tokens completely', () => {
    const rawJwt = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c';

    // Standalone token
    const standaloneSanitized = redactSensitiveData(`Bearer ${rawJwt}`);
    expect(standaloneSanitized).not.toContain(rawJwt);
    expect(standaloneSanitized).toContain('[REDACTED_JWT]');

    // Query param token
    const urlInput = `Connecting to ws://localhost:7880/rtc?access_token=${rawJwt}`;
    const urlSanitized = redactSensitiveData(urlInput);
    expect(urlSanitized).not.toContain(rawJwt);
    expect(urlSanitized).toContain('access_token=[REDACTED]');
  });

  it('redacts sensitive fields like ice_pwd, secret, and token', () => {
    const input = JSON.stringify({
      room: 'test-room',
      access_token: 'secret-token-12345',
      ice_pwd: 'passw0rd!IcePassword',
      secret: 'my-ultra-secret',
      credential: 'top-secret-turn-credential'
    });

    const sanitized = redactSensitiveData(input);
    expect(sanitized).not.toContain('secret-token-12345');
    expect(sanitized).not.toContain('passw0rd!IcePassword');
    expect(sanitized).not.toContain('my-ultra-secret');
    expect(sanitized).not.toContain('top-secret-turn-credential');
  });

  it('redacts SDP payload unless OBS_LOG_RAW_SDP=true and debug level is active', () => {
    const sdpPayload = 'v=0\r\no=- 123456 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\n';

    // Disallowed by default
    const sanitizedDefault = sanitizePayload(sdpPayload, false, false);
    expect(sanitizedDefault).toContain('[SDP_PAYLOAD_REDACTED');
    expect(sanitizedDefault).not.toContain('m=audio 9');

    // Allowed when flag and debug level are true
    const sanitizedAllowed = sanitizePayload(sdpPayload, true, true);
    expect(sanitizedAllowed).toContain('m=audio 9 UDP/TLS/RTP/SAVPF 111');
  });
});
