/**
 * Security and Data Sanitization Utility
 * Redacts sensitive tokens, credentials, ICE passwords, and unapproved SDP payloads.
 */

const SENSITIVE_KEY_PATTERNS = [
  /("?(?:access_token|token|secret|ice_pwd|credential|password|private_key)"?\s*[:=]\s*)"([^"]+)"/gi,
  /("?(?:access_token|token|secret|ice_pwd|credential|password|private_key)"?\s*[:=]\s*)'([^']+)'/gi,
  /("?(?:access_token|token|secret|ice_pwd|credential|password|private_key)"?\s*[:=]\s*)([^\s,;&{}]+)/gi,
  /(access_token=)([^&]+)/gi
];

const JWT_PATTERN = /eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}/g;

export function redactSensitiveData(input: string): string {
  if (!input) return input;

  let sanitized = input;

  // Redact complete JWT structures
  sanitized = sanitized.replace(JWT_PATTERN, '[REDACTED_JWT]');

  // Redact key-value pairs matching sensitive fields
  for (const pattern of SENSITIVE_KEY_PATTERNS) {
    sanitized = sanitized.replace(pattern, '$1[REDACTED]');
  }

  return sanitized;
}

export function sanitizePayload(payload: unknown, allowRawSdp = false, isDebugLevel = false): string {
  if (payload === undefined || payload === null) return '';

  let str = typeof payload === 'string' ? payload : JSON.stringify(payload);

  // Redact SDP offer/answer text unless OBS_LOG_RAW_SDP=true and debug/trace level
  if (!allowRawSdp || !isDebugLevel) {
    if (str.includes('v=0') && str.includes('m=audio') || str.includes('m=video')) {
      str = '[SDP_PAYLOAD_REDACTED; set OBS_LOG_RAW_SDP=true and OBS_LOG_LEVEL=debug to view]';
    }
  }

  return redactSensitiveData(str);
}
