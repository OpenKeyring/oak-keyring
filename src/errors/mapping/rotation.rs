use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext, ErrorLevel};

#[derive(Debug, thiserror::Error)]
pub enum RotationError {
    #[error("offline: cannot rotate without network")]
    Offline,

    #[error("sync busy: could not pause sync within 30s")]
    SyncBusy,

    #[error("conflict detected: cloud version is {cloud_version}, local version is {local_version}")]
    ConflictDetected { cloud_version: u32, local_version: u32 },

    #[error("record migration failed for {record_id}: {reason}")]
    RecordMigrationFailed { record_id: String, reason: String },

    #[error("push failed: {0}")]
    PushFailed(String),

    #[error("checkpoint corrupted: {0}")]
    CheckpointCorrupted(String),

    #[error("max DEK version exceeded: current={current}, max={max}")]
    MaxVersionExceeded { current: u32, max: u32 },

    #[error("vault not unlocked")]
    VaultNotUnlocked,

    #[error("internal error: {0}")]
    Internal(String),
}

impl ServiceError for RotationError {
    fn error_code(&self) -> ErrorCode {
        ErrorCode::Rotation(self.to_string())
    }

    fn error_context(&self) -> Option<ErrorContext> {
        None
    }

    fn error_level(&self) -> ErrorLevel {
        match self {
            RotationError::Offline => ErrorLevel::Warning,
            RotationError::SyncBusy => ErrorLevel::Error,
            RotationError::ConflictDetected { .. } => ErrorLevel::Warning,
            RotationError::RecordMigrationFailed { .. } => ErrorLevel::Error,
            RotationError::PushFailed(_) => ErrorLevel::Error,
            RotationError::CheckpointCorrupted(_) => ErrorLevel::Fatal,
            RotationError::MaxVersionExceeded { .. } => ErrorLevel::Fatal,
            RotationError::VaultNotUnlocked => ErrorLevel::Fatal,
            RotationError::Internal(_) => ErrorLevel::Error,
        }
    }
}

impl From<RotationError> for crate::errors::ServiceErrorBox {
    fn from(err: RotationError) -> Self {
        Box::new(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_error_level_is_warning() {
        let err = RotationError::Offline;
        assert_eq!(err.error_level(), ErrorLevel::Warning);
    }

    #[test]
    fn sync_busy_error_level_is_error() {
        let err = RotationError::SyncBusy;
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn checkpoint_corrupted_error_level_is_fatal() {
        let err = RotationError::CheckpointCorrupted("json parse error".into());
        assert_eq!(err.error_level(), ErrorLevel::Fatal);
    }

    #[test]
    fn max_version_exceeded_error_level_is_fatal() {
        let err = RotationError::MaxVersionExceeded { current: 9999, max: 10000 };
        assert_eq!(err.error_level(), ErrorLevel::Fatal);
    }

    #[test]
    fn rotation_error_converts_to_service_error_box() {
        let err = RotationError::Offline;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.error_level(), ErrorLevel::Warning);
    }

    #[test]
    fn rotation_error_code_is_rotation_variant() {
        let err = RotationError::Internal("test".into());
        assert!(matches!(err.error_code(), ErrorCode::Rotation(_)));
    }
}
