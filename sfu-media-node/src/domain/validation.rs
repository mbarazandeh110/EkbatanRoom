use crate::domain::error::SfuError;
use mediasoup::prelude::*;
use mediasoup::types::data_structures::{DtlsFingerprint, DtlsRole};

pub fn validate_not_empty(field_name: &str, value: &str) -> Result<(), SfuError> {
    if value.trim().is_empty() {
        Err(SfuError::InvalidArgument(format!(
            "Field '{field_name}' cannot be empty"
        )))
    } else {
        Ok(())
    }
}

pub fn parse_media_kind(kind: &str) -> Result<MediaKind, SfuError> {
    match kind.to_lowercase().as_str() {
        "audio" => Ok(MediaKind::Audio),
        "video" => Ok(MediaKind::Video),
        other => Err(SfuError::InvalidArgument(format!(
            "Invalid media kind '{other}', expected 'audio' or 'video'"
        ))),
    }
}

pub fn parse_dtls_role(role: &str) -> Result<DtlsRole, SfuError> {
    match role.to_lowercase().as_str() {
        "auto" => Ok(DtlsRole::Auto),
        "client" => Ok(DtlsRole::Client),
        "server" => Ok(DtlsRole::Server),
        other => Err(SfuError::InvalidArgument(format!(
            "Invalid dtls_role '{other}', expected 'auto', 'client', or 'server'"
        ))),
    }
}

pub fn parse_rtp_parameters(json_str: &str) -> Result<RtpParameters, SfuError> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return Err(SfuError::InvalidArgument(
            "rtp_parameters_json cannot be empty".to_string(),
        ));
    }

    let params: RtpParameters = serde_json::from_str(trimmed)
        .map_err(|e| SfuError::InvalidArgument(format!("Invalid rtp_parameters_json: {e}")))?;

    if params.codecs.is_empty() {
        return Err(SfuError::InvalidArgument(
            "rtp_parameters must include at least one codec".to_string(),
        ));
    }

    Ok(params)
}

pub fn parse_rtp_capabilities(json_str: &str) -> Result<RtpCapabilities, SfuError> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return Err(SfuError::InvalidArgument(
            "rtp_capabilities_json cannot be empty".to_string(),
        ));
    }

    let caps: RtpCapabilities = serde_json::from_str(trimmed)
        .map_err(|e| SfuError::InvalidArgument(format!("Invalid rtp_capabilities_json: {e}")))?;

    Ok(caps)
}

pub fn parse_dtls_fingerprints(
    fingerprints: &[(String, String)],
) -> Result<Vec<DtlsFingerprint>, SfuError> {
    if fingerprints.is_empty() {
        return Err(SfuError::InvalidArgument(
            "At least one DTLS fingerprint is required".to_string(),
        ));
    }

    let mut parsed = Vec::with_capacity(fingerprints.len());
    for (algorithm, value) in fingerprints {
        let json_val = serde_json::json!({
            "algorithm": algorithm.to_lowercase(),
            "value": value
        });
        let fp: DtlsFingerprint = serde_json::from_value(json_val).map_err(|e| {
            SfuError::InvalidArgument(format!(
                "Invalid DTLS fingerprint with algorithm '{algorithm}': {e}"
            ))
        })?;
        parsed.push(fp);
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_media_kind() {
        assert_eq!(parse_media_kind("audio").unwrap(), MediaKind::Audio);
        assert_eq!(parse_media_kind("AUDIO").unwrap(), MediaKind::Audio);
        assert_eq!(parse_media_kind("video").unwrap(), MediaKind::Video);
        assert!(parse_media_kind("data").is_err());
    }

    #[test]
    fn test_parse_dtls_role() {
        assert_eq!(parse_dtls_role("auto").unwrap(), DtlsRole::Auto);
        assert_eq!(parse_dtls_role("client").unwrap(), DtlsRole::Client);
        assert_eq!(parse_dtls_role("server").unwrap(), DtlsRole::Server);
        assert!(parse_dtls_role("unknown").is_err());
    }

    #[test]
    fn test_parse_invalid_rtp_parameters() {
        assert!(parse_rtp_parameters("").is_err());
        assert!(parse_rtp_parameters("not json").is_err());
        assert!(parse_rtp_parameters(r#"{"codecs":[]}"#).is_err());
    }

    #[test]
    fn test_parse_valid_rtp_capabilities() {
        let json = r#"{
            "codecs": [
                {
                    "mimeType": "audio/opus",
                    "kind": "audio",
                    "clockRate": 48000,
                    "channels": 2,
                    "preferredPayloadType": 111,
                    "rtcpFeedback": []
                }
            ],
            "headerExtensions": []
        }"#;

        let caps = parse_rtp_capabilities(json).unwrap();
        assert_eq!(caps.codecs.len(), 1);
    }

    #[test]
    fn test_parse_dtls_fingerprints() {
        let valid_sha256 = "E4:87:72:DE:DE:D6:75:A5:C0:5E:2B:A3:8F:69:B8:31:84:87:C7:E2:03:7E:63:F6:FD:E5:5D:84:1F:B5:1D:93";
        let fps = vec![("sha-256".to_string(), valid_sha256.to_string())];
        let result = parse_dtls_fingerprints(&fps).unwrap();
        assert_eq!(result.len(), 1);

        // Invalid algorithm
        let invalid = vec![("sha-999".to_string(), "abc".to_string())];
        assert!(parse_dtls_fingerprints(&invalid).is_err());
    }
}
