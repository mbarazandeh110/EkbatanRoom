use crate::domain::errors::SfuDomainError;
use mediasoup::prelude::*;
use mediasoup::types::data_structures::{DtlsFingerprint, DtlsRole};

pub fn validate_not_empty(field_name: &str, value: &str) -> Result<(), SfuDomainError> {
    if value.trim().is_empty() {
        Err(SfuDomainError::InvalidArgument(format!(
            "Field '{field_name}' cannot be empty"
        )))
    } else {
        Ok(())
    }
}

pub fn parse_media_kind(kind: &str) -> Result<MediaKind, SfuDomainError> {
    match kind.to_lowercase().as_str() {
        "audio" => Ok(MediaKind::Audio),
        "video" => Ok(MediaKind::Video),
        other => Err(SfuDomainError::InvalidArgument(format!(
            "Invalid media kind '{other}', expected 'audio' or 'video'"
        ))),
    }
}

pub fn parse_dtls_role(role: &str) -> Result<DtlsRole, SfuDomainError> {
    match role.to_lowercase().as_str() {
        "auto" => Ok(DtlsRole::Auto),
        "client" => Ok(DtlsRole::Client),
        "server" => Ok(DtlsRole::Server),
        other => Err(SfuDomainError::InvalidArgument(format!(
            "Invalid dtls_role '{other}', expected 'auto', 'client', or 'server'"
        ))),
    }
}

pub fn parse_rtp_parameters(json_str: &str) -> Result<RtpParameters, SfuDomainError> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return Err(SfuDomainError::InvalidArgument(
            "rtp_parameters_json cannot be empty".to_string(),
        ));
    }

    let params: RtpParameters = serde_json::from_str(trimmed).map_err(|e| {
        SfuDomainError::InvalidArgument(format!("Invalid rtp_parameters_json: {e}"))
    })?;

    if params.codecs.is_empty() {
        return Err(SfuDomainError::InvalidArgument(
            "rtp_parameters must include at least one codec".to_string(),
        ));
    }

    Ok(params)
}

pub fn parse_rtp_capabilities(json_str: &str) -> Result<RtpCapabilities, SfuDomainError> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return Err(SfuDomainError::InvalidArgument(
            "rtp_capabilities_json cannot be empty".to_string(),
        ));
    }

    let caps: RtpCapabilities = serde_json::from_str(trimmed).map_err(|e| {
        SfuDomainError::InvalidArgument(format!("Invalid rtp_capabilities_json: {e}"))
    })?;

    if caps.codecs.is_empty() {
        return Err(SfuDomainError::InvalidArgument(
            "rtp_capabilities must include at least one codec".to_string(),
        ));
    }

    Ok(caps)
}

pub fn parse_dtls_fingerprints(
    fingerprints: &[(String, String)],
) -> Result<Vec<DtlsFingerprint>, SfuDomainError> {
    if fingerprints.is_empty() {
        return Err(SfuDomainError::InvalidArgument(
            "At least one DTLS fingerprint is required".to_string(),
        ));
    }

    let mut result = Vec::with_capacity(fingerprints.len());
    for (algorithm, value) in fingerprints {
        let json_val = serde_json::json!({
            "algorithm": algorithm.to_lowercase(),
            "value": value
        });
        let fp: DtlsFingerprint = serde_json::from_value(json_val).map_err(|e| {
            SfuDomainError::InvalidArgument(format!(
                "Invalid DTLS fingerprint with algorithm '{algorithm}': {e}"
            ))
        })?;
        result.push(fp);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_not_empty() {
        assert!(validate_not_empty("req_id", "abc").is_ok());
        assert!(validate_not_empty("req_id", "  ").is_err());
        assert!(validate_not_empty("req_id", "").is_err());
    }

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
        assert!(parse_dtls_role("invalid").is_err());
    }
}
