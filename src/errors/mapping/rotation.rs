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

impl ServiceError for RotationError {
    fn to_error_code(&self) -> ErrorCode {
        match self {
            // Offline/SyncBusy are sync-related issues
            RotationError::Offline => ErrorCode::SyncNetworkUnreachable,
            RotationError::SyncBusy => ErrorCode::SyncProviderError,
            RotationError::ConflictDetected { .. } => ErrorCode::SyncConflictDetected,

            // Rotation-specific errors
            RotationError::RecordMigrationFailed { .. } => ErrorCode::RotationRecordMigrationFailed,
            RotationError::PushFailed(_) => ErrorCode::RotationPushFailed,
            RotationError::CheckpointCorrupted(_) => ErrorCode::RotationCheckpointCorrupted,
            RotationError::MaxVersionExceeded { .. } => ErrorCode::RotationMaxVersionExceeded,
            RotationError::Internal(_) => ErrorCode::RotationInternalError,

            // Vault not unlocked — use the vault variant directly
            RotationError::VaultNotUnlocked => ErrorCode::VaultNotUnlocked,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        match self {
            RotationError::ConflictDetected {
                cloud_version,
                local_version,
            } => ErrorContext::new()
                .expected_version(*local_version as u64)
                .actual_version(*cloud_version as u64),
            RotationError::RecordMigrationFailed { record_id, .. } => {
                ErrorContext::new().field_name(record_id)
            }
            RotationError::MaxVersionExceeded { current, max } => ErrorContext::new()
                .actual_version(*current as u64)
                .expected_version(*max as u64),
            _ => ErrorContext::new(),
        }
    }

    fn to_fallback_message(&self) -> String {
        self.to_string()
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
    fn offline_is_sync_network_unreachable() {
        let err = RotationError::Offline;
        assert_eq!(err.to_error_code(), ErrorCode::SyncNetworkUnreachable);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
    }

    #[test]
    fn sync_busy_is_sync_provider_error() {
        let err = RotationError::SyncBusy;
        assert_eq!(err.to_error_code(), ErrorCode::SyncProviderError);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
    }

    #[test]
    fn conflict_detected_is_sync_conflict_detected() {
        let err = RotationError::ConflictDetected {
            cloud_version: 7,
            local_version: 5,
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncConflictDetected);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
        let ctx = err.to_error_context();
        assert_eq!(ctx.expected_version, Some(5));
        assert_eq!(ctx.actual_version, Some(7));
    }

    #[test]
    fn record_migration_failed_is_rotation_specific() {
        let err = RotationError::RecordMigrationFailed {
            record_id: "rec_001".to_string(),
            reason: "bad key".to_string(),
        };
        assert_eq!(
            err.to_error_code(),
            ErrorCode::RotationRecordMigrationFailed
        );
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("rec_001".to_string()));
    }

    #[test]
    fn checkpoint_corrupted_is_fatal() {
        let err = RotationError::CheckpointCorrupted("json parse error".into());
        assert_eq!(err.to_error_code(), ErrorCode::RotationCheckpointCorrupted);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Fatal
        );
    }

    #[test]
    fn max_version_exceeded_is_operation() {
        let err = RotationError::MaxVersionExceeded {
            current: 9999,
            max: 10000,
        };
        assert_eq!(err.to_error_code(), ErrorCode::RotationMaxVersionExceeded);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
        let ctx = err.to_error_context();
        assert_eq!(ctx.actual_version, Some(9999));
        assert_eq!(ctx.expected_version, Some(10000));
    }

    #[test]
    fn vault_not_unlocked_is_vault_variant() {
        let err = RotationError::VaultNotUnlocked;
        assert_eq!(err.to_error_code(), ErrorCode::VaultNotUnlocked);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn push_failed_is_rotation_specific() {
        let err = RotationError::PushFailed("network error".into());
        assert_eq!(err.to_error_code(), ErrorCode::RotationPushFailed);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn rotation_error_converts_to_service_error_box() {
        let err = RotationError::Offline;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code(), ErrorCode::SyncNetworkUnreachable);
    }

    #[test]
    fn internal_error_is_rotation_specific() {
        let err = RotationError::Internal("test".into());
        assert_eq!(err.to_error_code(), ErrorCode::RotationInternalError);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }
}
