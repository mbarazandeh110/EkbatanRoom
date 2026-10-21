use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SfuError {
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Idempotency conflict: {0}")]
    IdempotencyConflict(String),

    #[error("Request is in-flight")]
    InFlight,

    #[error("Incompatible codec/capabilities: {0}")]
    IncompatibleCodec(String),

    #[error("Mediasoup error: {0}")]
    Mediasoup(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<SfuError> for tonic::Status {
    fn from(err: SfuError) -> Self {
        match err {
            SfuError::InvalidArgument(msg) => tonic::Status::invalid_argument(msg),
            SfuError::NotFound(msg) => tonic::Status::not_found(msg),
            SfuError::AlreadyExists(msg) => tonic::Status::already_exists(msg),
            SfuError::IdempotencyConflict(msg) => tonic::Status::already_exists(msg),
            SfuError::InFlight => tonic::Status::aborted("Operation is already in-flight"),
            SfuError::IncompatibleCodec(msg) => tonic::Status::failed_precondition(msg),
            SfuError::Mediasoup(msg) => tonic::Status::internal(format!("Mediasoup error: {msg}")),
            SfuError::Internal(msg) => tonic::Status::internal(msg),
        }
    }
}
