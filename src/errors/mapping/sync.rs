use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext};

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

    #[error("Cancelled: {operation}")]
    Cancelled { operation: String },
}

impl ServiceError for SyncError {
    fn to_error_code(&self) -> ErrorCode {
        match self {
            SyncError::NetworkTimeout { .. } => ErrorCode::SyncConnectionTimeout,
            SyncError::NetworkUnreachable { .. } => ErrorCode::SyncNetworkUnreachable,
            SyncError::ConnectionRefused { .. } => ErrorCode::SyncConnectionTimeout,
            SyncError::AuthenticationFailed { .. } | SyncError::TokenExpired => {
                ErrorCode::SyncAuthenticationFailed
            }
            SyncError::ChecksumMismatch { .. } | SyncError::AadInconsistent { .. } => {
                ErrorCode::SyncMetadataCorrupted
            }
            SyncError::SerializationFailed { .. } | SyncError::DeserializationFailed { .. } => {
                ErrorCode::SyncMetadataCorrupted
            }
            SyncError::InvalidStateTransition { .. } => ErrorCode::SyncProviderError,
            SyncError::LockAcquireFailed { .. } | SyncError::LockReleaseFailed { .. } => {
                ErrorCode::SyncProviderError
            }
            SyncError::ProviderNotSupported { .. } | SyncError::ProviderError { .. } => {
                ErrorCode::SyncProviderError
            }
            SyncError::ConfigValidationFailed { .. } => ErrorCode::SyncProviderError,
            SyncError::VaultIdentityMismatch { .. } => ErrorCode::SyncVaultIdentityMismatch,
            SyncError::MetadataVersionConflict { .. } => ErrorCode::SyncConflictDetected,
            SyncError::RecordNotFound { .. } => ErrorCode::VaultRecordNotFound,
            SyncError::PermissionDenied { .. } => ErrorCode::SyncPermissionDenied,
            SyncError::QuotaExceeded { .. } => ErrorCode::SyncDiskFull,
            SyncError::Cancelled { .. } => ErrorCode::SyncProviderError,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        match self {
            SyncError::NetworkTimeout { message } => {
                ErrorContext::new().extra("message", message.clone())
            }
            SyncError::NetworkUnreachable { message } => {
                ErrorContext::new().extra("message", message.clone())
            }
            SyncError::ConnectionRefused { endpoint } => {
                ErrorContext::new().extra("endpoint", endpoint.clone())
            }
            SyncError::AuthenticationFailed { reason } => {
                ErrorContext::new().extra("reason", reason.clone())
            }
            SyncError::TokenExpired => ErrorContext::new(),
            SyncError::ChecksumMismatch {
                expected,
                actual,
                record_id,
            } => ErrorContext::new()
                .field_name(record_id.clone())
                .extra("expected", expected.clone())
                .extra("actual", actual.clone()),
            SyncError::AadInconsistent {
                field,
                expected,
                actual,
            } => ErrorContext::new()
                .field_name(field.clone())
                .extra("expected", expected.clone())
                .extra("actual", actual.clone()),
            SyncError::SerializationFailed { message } => {
                ErrorContext::new().extra("message", message.clone())
            }
            SyncError::DeserializationFailed { message } => {
                ErrorContext::new().extra("message", message.clone())
            }
            SyncError::InvalidStateTransition { from, to } => ErrorContext::new()
                .extra("from", from.clone())
                .extra("to", to.clone()),
            SyncError::LockAcquireFailed { reason } => {
                ErrorContext::new().extra("reason", reason.clone())
            }
            SyncError::LockReleaseFailed { reason } => {
                ErrorContext::new().extra("reason", reason.clone())
            }
            SyncError::ProviderNotSupported { provider } => {
                ErrorContext::new().provider_name(provider.clone())
            }
            SyncError::ProviderError { provider, message } => ErrorContext::new()
                .provider_name(provider.clone())
                .extra("message", message.clone()),
            SyncError::ConfigValidationFailed { field, reason } => ErrorContext::new()
                .field_name(field.clone())
                .extra("reason", reason.clone()),
            SyncError::VaultIdentityMismatch {
                local_token,
                remote_token,
            } => ErrorContext::new()
                .extra("local_token", local_token.clone())
                .extra("remote_token", remote_token.clone()),
            SyncError::MetadataVersionConflict { local, remote } => ErrorContext::new()
                .expected_version(*local)
                .actual_version(*remote),
            SyncError::RecordNotFound { record_id } => {
                ErrorContext::new().field_name(record_id.clone())
            }
            SyncError::PermissionDenied { path } => {
                ErrorContext::new().file_path(std::path::PathBuf::from(path))
            }
            SyncError::QuotaExceeded { provider } => {
                ErrorContext::new().provider_name(provider.clone())
            }
            SyncError::Cancelled { operation } => {
                ErrorContext::new().extra("operation", operation.clone())
            }
        }
    }

    fn to_fallback_message(&self) -> String {
        self.to_string()
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
    use crate::errors::ErrorLevel;

    #[test]
    fn sync_error_network_timeout() {
        let err = SyncError::NetworkTimeout {
            message: "connection timed out".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncConnectionTimeout);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn sync_error_network_unreachable() {
        let err = SyncError::NetworkUnreachable {
            message: "host unreachable".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncNetworkUnreachable);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn sync_error_connection_refused() {
        let err = SyncError::ConnectionRefused {
            endpoint: "localhost:8080".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncConnectionTimeout);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn sync_error_authentication_failed() {
        let err = SyncError::AuthenticationFailed {
            reason: "invalid credentials".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncAuthenticationFailed);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn sync_error_token_expired() {
        let err = SyncError::TokenExpired;
        assert_eq!(err.to_error_code(), ErrorCode::SyncAuthenticationFailed);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn sync_error_checksum_mismatch() {
        let err = SyncError::ChecksumMismatch {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
            record_id: "rec_001".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncMetadataCorrupted);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
        let ctx = err.to_error_context();
        assert_eq!(ctx.extra.get("expected"), Some(&"abc123".to_string()));
        assert_eq!(ctx.extra.get("actual"), Some(&"def456".to_string()));
    }

    #[test]
    fn sync_error_aad_inconsistent() {
        let err = SyncError::AadInconsistent {
            field: "password".to_string(),
            expected: "v1".to_string(),
            actual: "v2".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncMetadataCorrupted);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn sync_error_serialization_failed() {
        let err = SyncError::SerializationFailed {
            message: "JSON error".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncMetadataCorrupted);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn sync_error_deserialization_failed() {
        let err = SyncError::DeserializationFailed {
            message: "JSON error".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncMetadataCorrupted);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn sync_error_invalid_state_transition() {
        let err = SyncError::InvalidStateTransition {
            from: "syncing".to_string(),
            to: "locked".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncProviderError);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn sync_error_lock_acquire_failed() {
        let err = SyncError::LockAcquireFailed {
            reason: "timeout".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncProviderError);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn sync_error_lock_release_failed() {
        let err = SyncError::LockReleaseFailed {
            reason: "not locked".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncProviderError);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn sync_error_provider_not_supported() {
        let err = SyncError::ProviderNotSupported {
            provider: "unknown".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncProviderError);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
        let ctx = err.to_error_context();
        assert_eq!(ctx.provider_name, Some("unknown".to_string()));
    }

    #[test]
    fn sync_error_provider_error() {
        let err = SyncError::ProviderError {
            provider: "s3".to_string(),
            message: "access denied".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncProviderError);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
        let ctx = err.to_error_context();
        assert_eq!(ctx.provider_name, Some("s3".to_string()));
    }

    #[test]
    fn sync_error_config_validation_failed() {
        let err = SyncError::ConfigValidationFailed {
            field: "endpoint".to_string(),
            reason: "invalid URL".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncProviderError);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn sync_error_vault_identity_mismatch() {
        let err = SyncError::VaultIdentityMismatch {
            local_token: "local_abc".to_string(),
            remote_token: "remote_xyz".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncVaultIdentityMismatch);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Fatal);
        let ctx = err.to_error_context();
        assert_eq!(ctx.extra.get("local_token"), Some(&"local_abc".to_string()));
        assert_eq!(
            ctx.extra.get("remote_token"),
            Some(&"remote_xyz".to_string())
        );
    }

    #[test]
    fn sync_error_metadata_version_conflict() {
        let err = SyncError::MetadataVersionConflict {
            local: 5,
            remote: 7,
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncConflictDetected);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
        let ctx = err.to_error_context();
        assert_eq!(ctx.expected_version, Some(5));
        assert_eq!(ctx.actual_version, Some(7));
    }

    #[test]
    fn sync_error_record_not_found() {
        let err = SyncError::RecordNotFound {
            record_id: "rec_999".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::VaultRecordNotFound);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn sync_error_permission_denied() {
        let err = SyncError::PermissionDenied {
            path: "/vault/records".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncPermissionDenied);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn sync_error_quota_exceeded() {
        let err = SyncError::QuotaExceeded {
            provider: "dropbox".to_string(),
        };
        assert_eq!(err.to_error_code(), ErrorCode::SyncDiskFull);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
        let ctx = err.to_error_context();
        assert_eq!(ctx.provider_name, Some("dropbox".to_string()));
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
