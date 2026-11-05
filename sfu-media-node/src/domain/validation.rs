use crate::domain::entities::{DtlsRole, MediaKind};
use crate::domain::errors::SfuDomainError;

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
    match kind.trim().to_lowercase().as_str() {
        "audio" => Ok(MediaKind::Audio),
        "video" => Ok(MediaKind::Video),
        other => Err(SfuDomainError::InvalidArgument(format!(
            "Invalid media kind '{other}', expected 'audio' or 'video'"
        ))),
    }
}

pub fn parse_dtls_role(role: &str) -> Result<DtlsRole, SfuDomainError> {
    match role.trim().to_lowercase().as_str() {
        "auto" | "" => Ok(DtlsRole::Auto),
        "client" => Ok(DtlsRole::Client),
        "server" => Ok(DtlsRole::Server),
        other => Err(SfuDomainError::InvalidArgument(format!(
            "Invalid dtls_role '{other}', expected 'auto', 'client', or 'server'"
        ))),
    }
}

pub fn parse_dtls_fingerprints(
    fingerprints: &[(String, String)],
) -> Result<Vec<mediasoup::types::data_structures::DtlsFingerprint>, SfuDomainError> {
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
        let fp: mediasoup::types::data_structures::DtlsFingerprint =
            serde_json::from_value(json_val).map_err(|e| {
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
        assert_eq!(parse_dtls_role("").unwrap(), DtlsRole::Auto);
        assert!(parse_dtls_role("invalid").is_err());
    }

    #[test]
    fn test_parse_dtls_fingerprints() {
        let fps = vec![(
            "sha-256".to_string(),
            "E4:87:72:DE:DE:D6:75:A5:C0:5E:2B:A3:8F:69:B8:31:84:87:C7:E2:03:7E:63:F6:FD:E5:5D:84:1F:B5:1D:93".to_string(),
        )];
        let parsed = parse_dtls_fingerprints(&fps).unwrap();
        assert_eq!(parsed.len(), 1);

        assert!(parse_dtls_fingerprints(&[]).is_err());
    }
}
