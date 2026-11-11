use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SfuDomainError {
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Resource already exists: {0}")]
    AlreadyExists(String),

    #[error("Incompatible codec: {0}")]
    IncompatibleCodec(String),

    #[error("Idempotency conflict: {0}")]
    IdempotencyConflict(String),

    #[error("Operation currently in flight")]
    InFlight,

    #[error("Mediasoup error: {0}")]
    Mediasoup(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Node is draining: {0}")]
    SystemDraining(String),

    #[error("Enterprise feature required: {0}")]
    EnterpriseFeatureRequired(String),

    #[error("License permission denied: {0}")]
    LicensePermissionDenied(String),

    #[error("Invalid license: {0}")]
    InvalidLicense(String),

    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
}
