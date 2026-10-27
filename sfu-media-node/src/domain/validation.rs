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
}
