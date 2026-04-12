use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext, ErrorLevel};

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Network timeout: {message}")]
    NetworkTimeout { message: String },

    #[error("Network unreachable: {message}")]
    NetworkUnreachable { message: String },

    #[error("Connection refused: {endpoint}")]
    ConnectionRefused { endpoint: String },

    #[error("Authentication failed: {reason}")]
    AuthenticationFailed { reason: String },

    #[error("OAuth token expired")]
    TokenExpired,

    #[error("Checksum mismatch: expected {expected}, actual {actual} for record {record_id}")]
    ChecksumMismatch {
        expected: String,
        actual: String,
        record_id: String,
    },

    #[error("AAD inconsistent for field '{field}': expected {expected}, actual {actual}")]
    AadInconsistent {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("Serialization failed: {message}")]
    SerializationFailed { message: String },

    #[error("Deserialization failed: {message}")]
    DeserializationFailed { message: String },

    #[error("Invalid state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Lock acquire failed: {reason}")]
    LockAcquireFailed { reason: String },

    #[error("Lock release failed: {reason}")]
    LockReleaseFailed { reason: String },

    #[error("Provider not supported: {provider}")]
    ProviderNotSupported { provider: String },

    #[error("Provider error [{provider}]: {message}")]
    ProviderError { provider: String, message: String },

    #[error("Config validation failed for '{field}': {reason}")]
    ConfigValidationFailed { field: String, reason: String },

    #[error("Vault identity mismatch: local={local_token}, remote={remote_token}")]
    VaultIdentityMismatch {
        local_token: String,
        remote_token: String,
    },

    #[error("Metadata version conflict: local={local}, remote={remote}")]
    MetadataVersionConflict { local: u64, remote: u64 },

    #[error("Record not found: {record_id}")]
    RecordNotFound { record_id: String },

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("Quota exceeded: {provider}")]
    QuotaExceeded { provider: String },
}

impl ServiceError for SyncError {
    fn error_code(&self) -> ErrorCode {
        ErrorCode::Sync(self.to_string())
    }

    fn error_context(&self) -> Option<ErrorContext> {
        match self {
            Self::ChecksumMismatch {
                expected,
                actual,
                record_id,
            } => Some(
                ErrorContext::new()
                    .with("expected", expected)
                    .with("actual", actual)
                    .with("record_id", record_id),
            ),
            Self::AadInconsistent {
                field,
                expected,
                actual,
            } => Some(
                ErrorContext::new()
                    .with("field", field)
                    .with("expected", expected)
                    .with("actual", actual),
            ),
            Self::InvalidStateTransition { from, to } => {
                Some(ErrorContext::new().with("from", from).with("to", to))
            }
            Self::VaultIdentityMismatch {
                local_token,
                remote_token,
            } => Some(
                ErrorContext::new()
                    .with("local_token", local_token)
                    .with("remote_token", remote_token),
            ),
            Self::MetadataVersionConflict { local, remote } => Some(
                ErrorContext::new()
                    .with("local", &local.to_string())
                    .with("remote", &remote.to_string()),
            ),
            Self::RecordNotFound { record_id } => {
                Some(ErrorContext::new().with("record_id", record_id))
            }
            Self::PermissionDenied { path } => Some(ErrorContext::new().with("path", path)),
            Self::NetworkTimeout { message } => Some(ErrorContext::new().with("message", message)),
            Self::NetworkUnreachable { message } => {
                Some(ErrorContext::new().with("message", message))
            }
            Self::ConnectionRefused { endpoint } => {
                Some(ErrorContext::new().with("endpoint", endpoint))
            }
            Self::AuthenticationFailed { reason } => {
                Some(ErrorContext::new().with("reason", reason))
            }
            Self::LockAcquireFailed { reason } => Some(ErrorContext::new().with("reason", reason)),
            Self::LockReleaseFailed { reason } => Some(ErrorContext::new().with("reason", reason)),
            Self::ProviderNotSupported { provider } => {
                Some(ErrorContext::new().with("provider", provider))
            }
            Self::ProviderError { provider, message } => Some(
                ErrorContext::new()
                    .with("provider", provider)
                    .with("message", message),
            ),
            Self::ConfigValidationFailed { field, reason } => Some(
                ErrorContext::new()
                    .with("field", field)
                    .with("reason", reason),
            ),
            Self::QuotaExceeded { provider } => {
                Some(ErrorContext::new().with("provider", provider))
            }
            Self::TokenExpired => None,
            Self::SerializationFailed { message } => {
                Some(ErrorContext::new().with("message", message))
            }
            Self::DeserializationFailed { message } => {
                Some(ErrorContext::new().with("message", message))
            }
        }
    }

    fn error_level(&self) -> ErrorLevel {
        match self {
            Self::NetworkTimeout { .. } => ErrorLevel::Warning,
            Self::NetworkUnreachable { .. } => ErrorLevel::Warning,
            Self::ConnectionRefused { .. } => ErrorLevel::Warning,
            Self::MetadataVersionConflict { .. } => ErrorLevel::Warning,
            Self::RecordNotFound { .. } => ErrorLevel::Warning,
            Self::VaultIdentityMismatch { .. } => ErrorLevel::Fatal,
            _ => ErrorLevel::Error,
        }
    }
}

impl From<SyncError> for crate::errors::ServiceErrorBox {
    fn from(err: SyncError) -> Self {
        Box::new(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::service_error::ServiceError;
    use crate::errors::{ErrorCode, ErrorLevel};

    #[test]
    fn sync_error_network_timeout() {
        let err = SyncError::NetworkTimeout {
            message: "connection timed out".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Warning);
        assert!(err.error_context().is_some());
        assert!(err.to_string().contains("connection timed out"));
    }

    #[test]
    fn sync_error_network_unreachable() {
        let err = SyncError::NetworkUnreachable {
            message: "host unreachable".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Warning);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_connection_refused() {
        let err = SyncError::ConnectionRefused {
            endpoint: "localhost:8080".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Warning);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_authentication_failed() {
        let err = SyncError::AuthenticationFailed {
            reason: "invalid credentials".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_token_expired() {
        let err = SyncError::TokenExpired;
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_none());
    }

    #[test]
    fn sync_error_checksum_mismatch() {
        let err = SyncError::ChecksumMismatch {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
            record_id: "rec_001".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
        let ctx = err.error_context().unwrap();
        assert_eq!(ctx.fields.get("expected"), Some(&"abc123".to_string()));
        assert_eq!(ctx.fields.get("actual"), Some(&"def456".to_string()));
        assert_eq!(ctx.fields.get("record_id"), Some(&"rec_001".to_string()));
    }

    #[test]
    fn sync_error_aad_inconsistent() {
        let err = SyncError::AadInconsistent {
            field: "password".to_string(),
            expected: "v1".to_string(),
            actual: "v2".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
        let ctx = err.error_context().unwrap();
        assert_eq!(ctx.fields.get("field"), Some(&"password".to_string()));
    }

    #[test]
    fn sync_error_serialization_failed() {
        let err = SyncError::SerializationFailed {
            message: "JSON error".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_deserialization_failed() {
        let err = SyncError::DeserializationFailed {
            message: "JSON error".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_invalid_state_transition() {
        let err = SyncError::InvalidStateTransition {
            from: "syncing".to_string(),
            to: "locked".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
        let ctx = err.error_context().unwrap();
        assert_eq!(ctx.fields.get("from"), Some(&"syncing".to_string()));
        assert_eq!(ctx.fields.get("to"), Some(&"locked".to_string()));
    }

    #[test]
    fn sync_error_lock_acquire_failed() {
        let err = SyncError::LockAcquireFailed {
            reason: "timeout".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_lock_release_failed() {
        let err = SyncError::LockReleaseFailed {
            reason: "not locked".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_provider_not_supported() {
        let err = SyncError::ProviderNotSupported {
            provider: "unknown".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_provider_error() {
        let err = SyncError::ProviderError {
            provider: "s3".to_string(),
            message: "access denied".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_config_validation_failed() {
        let err = SyncError::ConfigValidationFailed {
            field: "endpoint".to_string(),
            reason: "invalid URL".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_vault_identity_mismatch() {
        let err = SyncError::VaultIdentityMismatch {
            local_token: "local_abc".to_string(),
            remote_token: "remote_xyz".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Fatal);
        assert!(err.error_context().is_some());
        let ctx = err.error_context().unwrap();
        assert_eq!(
            ctx.fields.get("local_token"),
            Some(&"local_abc".to_string())
        );
        assert_eq!(
            ctx.fields.get("remote_token"),
            Some(&"remote_xyz".to_string())
        );
    }

    #[test]
    fn sync_error_metadata_version_conflict() {
        let err = SyncError::MetadataVersionConflict {
            local: 5,
            remote: 7,
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Warning);
        assert!(err.error_context().is_some());
        let ctx = err.error_context().unwrap();
        assert_eq!(ctx.fields.get("local"), Some(&"5".to_string()));
        assert_eq!(ctx.fields.get("remote"), Some(&"7".to_string()));
    }

    #[test]
    fn sync_error_record_not_found() {
        let err = SyncError::RecordNotFound {
            record_id: "rec_999".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Warning);
        assert!(err.error_context().is_some());
        let ctx = err.error_context().unwrap();
        assert_eq!(ctx.fields.get("record_id"), Some(&"rec_999".to_string()));
    }

    #[test]
    fn sync_error_permission_denied() {
        let err = SyncError::PermissionDenied {
            path: "/vault/records".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_quota_exceeded() {
        let err = SyncError::QuotaExceeded {
            provider: "dropbox".to_string(),
        };
        assert!(matches!(err.error_code(), ErrorCode::Sync(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.error_context().is_some());
    }

    #[test]
    fn sync_error_implements_service_error() {
        fn assert_service_error<E: ServiceError>(_: &E) {}
        assert_service_error(&SyncError::TokenExpired);
        assert_service_error(&SyncError::NetworkTimeout {
            message: "test".to_string(),
        });
        assert_service_error(&SyncError::ChecksumMismatch {
            expected: "a".to_string(),
            actual: "b".to_string(),
            record_id: "c".to_string(),
        });
        assert_service_error(&SyncError::VaultIdentityMismatch {
            local_token: "a".to_string(),
            remote_token: "b".to_string(),
        });
    }
}
