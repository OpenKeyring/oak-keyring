use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext};

#[derive(Debug, thiserror::Error)]
pub enum RotationError {
    #[error("offline: cannot rotate without network")]
    Offline,

    #[error("sync busy: could not pause sync within 30s")]
    SyncBusy,

    #[error(
        "conflict detected: cloud version is {cloud_version}, local version is {local_version}"
    )]
    ConflictDetected {
        cloud_version: u32,
        local_version: u32,
    },

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

impl From<RotationError> for crate::errors::ServiceErrorBox {
    fn from(err: RotationError) -> Self {
        Box::new(err)
    }
}
impl ServiceError for RotationError {
    fn to_error_code(&self) -> ErrorCode {
        match self {
            RotationError::Offline => ErrorCode::DekRotationFailed,
            RotationError::SyncBusy => ErrorCode::DekRotationFailed,
            RotationError::RecordMigrationFailed { .. } => ErrorCode::DekRotationFailed,
            RotationError::PushFailed(_) => ErrorCode::DekRotationFailed,
            RotationError::CheckpointCorrupted(_) => ErrorCode::DekRotationFailed,
            RotationError::MaxVersionExceeded { .. } => ErrorCode::DekRotationFailed,
            RotationError::Internal(_) => ErrorCode::DekRotationFailed,
            RotationError::ConflictDetected { .. } => ErrorCode::RotationConflictDetected,
            RotationError::VaultNotUnlocked => ErrorCode::ExecutorVaultLocked,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        match self {
            RotationError::ConflictDetected {
                cloud_version,
                local_version,
            } => ErrorContext::new()
                .expected_version(*cloud_version as u64)
                .actual_version(*local_version as u64),
            _ => ErrorContext::new(),
        }
    }

    fn to_fallback_message(&self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::ErrorLevel;

    #[test]
    fn offline_error_level_is_operation() {
        let err = RotationError::Offline;
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn sync_busy_error_level_is_operation() {
        let err = RotationError::SyncBusy;
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn checkpoint_corrupted_error_level_is_operation() {
        let err = RotationError::CheckpointCorrupted("json parse error".into());
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn max_version_exceeded_error_level_is_operation() {
        let err = RotationError::MaxVersionExceeded {
            current: 9999,
            max: 10000,
        };
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn rotation_error_converts_to_service_error_box() {
        let err = RotationError::Offline;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn rotation_error_code_is_dek_rotation_failed() {
        let err = RotationError::Internal("test".into());
        assert_eq!(err.to_error_code(), ErrorCode::DekRotationFailed);
    }
}
