use crate::domain::errors::SfuDomainError;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Default Ekbatan Root Public Key for Ed25519 offline license verification.
pub const DEFAULT_PUBLIC_KEY_HEX: &str =
    "23a1aa3536a3863f3bd6b468d2537192d9ec9cdcb127486738e145fe64eb5436";

/// Required feature flag in license claims for inter-node room cascading.
pub const FEATURE_INTER_NODE_CASCADING: &str = "inter_node_cascading";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterLimits {
    #[serde(default)]
    pub max_sfu_nodes: u32,
    #[serde(default)]
    pub max_concurrent_peers: u32,
    #[serde(default)]
    pub max_concurrent_rooms: u32,
    #[serde(default)]
    pub max_active_workers: u32,
    #[serde(default)]
    pub max_consumers_per_worker: u32,
}

impl Default for ClusterLimits {
    fn default() -> Self {
        Self {
            max_sfu_nodes: 1,
            max_concurrent_peers: 500,
            max_concurrent_rooms: 20,
            max_active_workers: 0,
            max_consumers_per_worker: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicenseClaims {
    pub license_id: String,
    pub customer_id: String,
    pub tier: String, // "community" | "enterprise"
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub cluster_limits: ClusterLimits,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    pub issued_at: u64, // Unix timestamp in seconds
    #[serde(default)]
    pub expires_at: u64, // Unix timestamp in seconds (0 = perpetual)
    #[serde(default)]
    pub grace_period_days: u32,
}

impl LicenseClaims {
    /// Returns true if the official expiration date has passed.
    #[must_use]
    pub fn is_expired_at(&self, now_secs: u64) -> bool {
        if self.expires_at == 0 {
            return false;
        }
        now_secs > self.expires_at
    }

    /// Returns true if currently within the grace period after expiration.
    #[must_use]
    pub fn is_in_grace_period_at(&self, now_secs: u64) -> bool {
        if self.expires_at == 0 {
            return false;
        }
        let grace_seconds = (self.grace_period_days as u64) * 86_400;
        now_secs > self.expires_at && now_secs <= self.expires_at.saturating_add(grace_seconds)
    }

    /// Returns true if the license is currently valid (not expired or within grace period).
    #[must_use]
    pub fn is_valid_at(&self, now_secs: u64) -> bool {
        if self.expires_at == 0 {
            return true;
        }
        if now_secs <= self.expires_at {
            return true;
        }
        self.is_in_grace_period_at(now_secs)
    }

    /// Checks if a specific feature is enabled in this license.
    #[must_use]
    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }

    /// Checks if this license grants enterprise cascading capabilities.
    #[must_use]
    pub fn is_enterprise_cascading_allowed(&self, now_secs: u64) -> bool {
        self.is_valid_at(now_secs)
            && (self.tier.eq_ignore_ascii_case("enterprise"))
            && self.has_feature(FEATURE_INTER_NODE_CASCADING)
    }

    /// Maximum active workers allowed by this license (0 = unlimited).
    #[must_use]
    pub fn max_active_workers(&self) -> u32 {
        self.cluster_limits.max_active_workers
    }

    /// Maximum consumers per worker allowed by this license (0 = unlimited).
    #[must_use]
    pub fn max_consumers_per_worker(&self) -> u32 {
        self.cluster_limits.max_consumers_per_worker
    }
}

/// Decodes base64 URL-safe with or without padding.
fn decode_b64_url(input: &str) -> Result<Vec<u8>, SfuDomainError> {
    URL_SAFE_NO_PAD
        .decode(input.trim())
        .or_else(|_| URL_SAFE.decode(input.trim()))
        .map_err(|e| SfuDomainError::InvalidLicense(format!("Base64 decoding failed: {e}")))
}

/// Parses a 32-byte public key from hex string.
pub fn parse_public_key_hex(hex_str: &str) -> Result<[u8; 32], SfuDomainError> {
    let bytes = hex::decode(hex_str.trim())
        .map_err(|e| SfuDomainError::InvalidLicense(format!("Invalid public key hex: {e}")))?;
    if bytes.len() != 32 {
        return Err(SfuDomainError::InvalidLicense(format!(
            "Public key must be exactly 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Verifies an Ed25519-signed license token in `EKB1.<b64_payload>.<b64_signature>` format.
pub fn verify_license_token(
    token: &str,
    public_key_bytes: &[u8; 32],
    now_secs: u64,
) -> Result<LicenseClaims, SfuDomainError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(SfuDomainError::InvalidLicense(
            "License token cannot be empty".to_string(),
        ));
    }

    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.len() != 3 {
        return Err(SfuDomainError::InvalidLicense(
            "License token must have format EKB1.<payload>.<signature>".to_string(),
        ));
    }

    if parts[0] != "EKB1" {
        return Err(SfuDomainError::InvalidLicense(format!(
            "Invalid license prefix '{}', expected 'EKB1'",
            parts[0]
        )));
    }

    let payload_b64 = parts[1];
    let signature_b64 = parts[2];

    let sig_bytes = decode_b64_url(signature_b64)?;
    if sig_bytes.len() != 64 {
        return Err(SfuDomainError::InvalidLicense(format!(
            "Invalid signature length: expected 64 bytes, got {}",
            sig_bytes.len()
        )));
    }

    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let signature = Signature::from_bytes(&sig_arr);

    let verifying_key = VerifyingKey::from_bytes(public_key_bytes)
        .map_err(|e| SfuDomainError::InvalidLicense(format!("Invalid Ed25519 public key: {e}")))?;

    // Signed data includes the exact header prefix to prevent header substitution
    let signed_data = format!("EKB1.{payload_b64}");
    verifying_key
        .verify(signed_data.as_bytes(), &signature)
        .map_err(|e| {
            SfuDomainError::InvalidLicense(format!("Cryptographic signature mismatch: {e}"))
        })?;

    let payload_bytes = decode_b64_url(payload_b64)?;
    let claims: LicenseClaims = serde_json::from_slice(&payload_bytes).map_err(|e| {
        SfuDomainError::InvalidLicense(format!("Failed to parse license claims JSON: {e}"))
    })?;

    if claims.is_expired_at(now_secs) {
        if claims.is_in_grace_period_at(now_secs) {
            let grace_end = claims
                .expires_at
                .saturating_add((claims.grace_period_days as u64) * 86_400);
            tracing::warn!(
                license_id = %claims.license_id,
                customer_id = %claims.customer_id,
                tier = %claims.tier,
                expired_at = claims.expires_at,
                grace_period_days = claims.grace_period_days,
                grace_ends_at = grace_end,
                "Enterprise license has expired and is operating within its grace period! Please renew immediately."
            );
        } else {
            return Err(SfuDomainError::InvalidLicense(format!(
                "License expired at unix timestamp {} and grace period of {} days has elapsed",
                claims.expires_at, claims.grace_period_days
            )));
        }
    }

    Ok(claims)
}

/// Generates a signed `EKB1.<b64_payload>.<b64_signature>` license token using an Ed25519 signing key.
pub fn generate_license_token(
    claims: &LicenseClaims,
    signing_key_bytes: &[u8; 32],
) -> Result<String, SfuDomainError> {
    let payload_json = serde_json::to_vec(claims).map_err(|e| {
        SfuDomainError::Internal(format!("Failed to serialize license claims: {e}"))
    })?;
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json);

    let signing_key = SigningKey::from_bytes(signing_key_bytes);
    let signed_data = format!("EKB1.{payload_b64}");
    let signature = signing_key.sign(signed_data.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    Ok(format!("EKB1.{payload_b64}.{sig_b64}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keys() -> ([u8; 32], [u8; 32]) {
        let priv_hex = "02dc0bfacd35d5f44baada453877d98d2dfadcfa175d67f211195a6eba102eb4";
        let pub_hex = "23a1aa3536a3863f3bd6b468d2537192d9ec9cdcb127486738e145fe64eb5436";

        let priv_bytes = hex::decode(priv_hex).unwrap();
        let pub_bytes = hex::decode(pub_hex).unwrap();

        let mut priv_arr = [0u8; 32];
        let mut pub_arr = [0u8; 32];
        priv_arr.copy_from_slice(&priv_bytes);
        pub_arr.copy_from_slice(&pub_bytes);

        (priv_arr, pub_arr)
    }

    #[test]
    fn test_valid_enterprise_license_verification() {
        let (priv_key, pub_key) = test_keys();
        let now = 1770000000;

        let claims = LicenseClaims {
            license_id: "lic-ent-001".to_string(),
            customer_id: "ekbatan-corp".to_string(),
            tier: "enterprise".to_string(),
            features: vec![FEATURE_INTER_NODE_CASCADING.to_string()],
            cluster_limits: ClusterLimits {
                max_sfu_nodes: 10,
                max_concurrent_peers: 5000,
                max_concurrent_rooms: 200,
                max_active_workers: 16,
                max_consumers_per_worker: 500,
            },
            allowed_domains: vec!["ekbatan.local".to_string()],
            issued_at: now - 1000,
            expires_at: now + 86400 * 30, // 30 days remaining
            grace_period_days: 7,
        };

        let token = generate_license_token(&claims, &priv_key).expect("token generation");
        assert!(token.starts_with("EKB1."));

        let verified = verify_license_token(&token, &pub_key, now).expect("verification");
        assert_eq!(verified.license_id, "lic-ent-001");
        assert_eq!(verified.tier, "enterprise");
        assert_eq!(verified.max_active_workers(), 16);
        assert_eq!(verified.max_consumers_per_worker(), 500);
        assert!(verified.is_enterprise_cascading_allowed(now));
        assert!(!verified.is_expired_at(now));
        assert!(!verified.is_in_grace_period_at(now));
    }

    #[test]
    fn test_expired_license_in_grace_period() {
        let (priv_key, pub_key) = test_keys();
        let now = 1770000000;

        let claims = LicenseClaims {
            license_id: "lic-grace-002".to_string(),
            customer_id: "corp".to_string(),
            tier: "enterprise".to_string(),
            features: vec![FEATURE_INTER_NODE_CASCADING.to_string()],
            cluster_limits: ClusterLimits::default(),
            allowed_domains: vec![],
            issued_at: now - 86400 * 40,
            expires_at: now - 86400 * 2, // Expired 2 days ago
            grace_period_days: 5,        // 5 days grace period
        };

        let token = generate_license_token(&claims, &priv_key).unwrap();
        let verified = verify_license_token(&token, &pub_key, now).expect("in grace period");

        assert!(verified.is_expired_at(now));
        assert!(verified.is_in_grace_period_at(now));
        assert!(verified.is_valid_at(now));
        assert!(verified.is_enterprise_cascading_allowed(now));
    }

    #[test]
    fn test_expired_license_after_grace_period_rejected() {
        let (priv_key, pub_key) = test_keys();
        let now = 1770000000;

        let claims = LicenseClaims {
            license_id: "lic-expired-003".to_string(),
            customer_id: "corp".to_string(),
            tier: "enterprise".to_string(),
            features: vec![FEATURE_INTER_NODE_CASCADING.to_string()],
            cluster_limits: ClusterLimits::default(),
            allowed_domains: vec![],
            issued_at: now - 86400 * 40,
            expires_at: now - 86400 * 10, // Expired 10 days ago
            grace_period_days: 5,         // Only 5 days grace period
        };

        let token = generate_license_token(&claims, &priv_key).unwrap();
        let err = verify_license_token(&token, &pub_key, now).unwrap_err();
        match err {
            SfuDomainError::InvalidLicense(msg) => {
                assert!(msg.contains("grace period of 5 days has elapsed"));
            }
            other => panic!("Unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_tampered_payload_rejected() {
        let (priv_key, pub_key) = test_keys();
        let now = 1770000000;

        let claims = LicenseClaims {
            license_id: "lic-004".to_string(),
            customer_id: "corp".to_string(),
            tier: "community".to_string(),
            features: vec![],
            cluster_limits: ClusterLimits::default(),
            allowed_domains: vec![],
            issued_at: now,
            expires_at: 0,
            grace_period_days: 0,
        };

        let token = generate_license_token(&claims, &priv_key).unwrap();
        let parts: Vec<&str> = token.split('.').collect();

        // Tamper payload by replacing it with another payload
        let tampered_claims = LicenseClaims {
            tier: "enterprise".to_string(),
            features: vec![FEATURE_INTER_NODE_CASCADING.to_string()],
            ..claims
        };
        let tampered_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&tampered_claims).unwrap());
        let tampered_token = format!("{}.{}.{}", parts[0], tampered_b64, parts[2]);

        let err = verify_license_token(&tampered_token, &pub_key, now).unwrap_err();
        match err {
            SfuDomainError::InvalidLicense(msg) => {
                assert!(msg.contains("Cryptographic signature mismatch"));
            }
            other => panic!("Unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_invalid_prefix_rejected() {
        let (_, pub_key) = test_keys();
        let err = verify_license_token("JWT.abc.def", &pub_key, 1770000000).unwrap_err();
        match err {
            SfuDomainError::InvalidLicense(msg) => {
                assert!(msg.contains("expected 'EKB1'"));
            }
            other => panic!("Unexpected error: {other:?}"),
        }
    }
}
