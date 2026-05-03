use std::path::PathBuf;

use uuid::Uuid;

use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext};

/// Errors that can occur during import/export operations.
///
/// Covers file I/O, decryption, parsing, validation, mapping,
/// encryption, session management, vault interaction, and general failures.
#[derive(Debug, thiserror::Error)]
pub enum ImportExportError {
    // -- File Errors --
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),

    #[error("file too large: {path} ({size} bytes, max {max})")]
    FileTooLarge {
        path: PathBuf,
        size: usize,
        max: usize,
    },

    #[error("failed to read file {path}: {reason}")]
    FileReadError { path: PathBuf, reason: String },

    #[error("failed to write file {path}: {reason}")]
    FileWriteError { path: PathBuf, reason: String },

    // -- Decryption Errors --
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("invalid password")]
    InvalidPassword,

    #[error("password required to decrypt this file")]
    PasswordRequired,

    // -- Parse Errors --
    #[error("failed to parse {format}: {reason}")]
    ParseError { format: String, reason: String },

    #[error("invalid format: {0}")]
    InvalidFormat(String),

    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    // -- Validation Errors --
    #[error("validation error on field '{field}': {reason}")]
    ValidationError { field: String, reason: String },

    #[error("missing required field: {0}")]
    MissingRequiredField(String),

    #[error("invalid type for field '{field}': expected {expected}, got {actual}")]
    InvalidFieldType {
        field: String,
        expected: String,
        actual: String,
    },

    // -- Mapping Errors --
    #[error("mapping error on field '{source_field}': {reason}")]
    MappingError {
        source_field: String,
        reason: String,
    },

    #[error("duplicate record '{name}': {reason}")]
    DuplicateRecord { name: String, reason: String },

    // -- Encryption Errors --
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("key derivation failed: {0}")]
    KeyDerivationFailed(String),

    // -- Session Errors --
    #[error("session not found: {0}")]
    SessionNotFound(Uuid),

    #[error("invalid session status: expected {expected}, got {actual}")]
    InvalidSessionStatus { expected: String, actual: String },

    #[error("session was cancelled")]
    SessionCancelled,

    // -- Vault Errors --
    #[error("vault error: {0}")]
    VaultError(String),

    // -- General Errors --
    #[error("internal error: {0}")]
    InternalError(String),

    #[error("operation timed out")]
    Timeout,
}

impl ServiceError for ImportExportError {
    fn to_error_code(&self) -> ErrorCode {
        match self {
            // File errors
            ImportExportError::FileNotFound(_) => ErrorCode::ImportFileUnreadable,
            ImportExportError::FileTooLarge { .. } => ErrorCode::ImportFileUnreadable,
            ImportExportError::FileReadError { .. } => ErrorCode::ImportFileUnreadable,
            ImportExportError::FileWriteError { .. } => ErrorCode::ExportWriteFailed,

            // Decryption errors
            ImportExportError::DecryptionFailed(_) => ErrorCode::ImportPasswordIncorrect,
            ImportExportError::InvalidPassword => ErrorCode::ImportPasswordIncorrect,
            ImportExportError::PasswordRequired => ErrorCode::ImportPasswordRequired,

            // Parse errors
            ImportExportError::ParseError { .. } => ErrorCode::ImportFileFormatInvalid,
            ImportExportError::InvalidFormat(_) => ErrorCode::ImportFileFormatInvalid,
            ImportExportError::UnsupportedFormat(_) => ErrorCode::ImportFileFormatInvalid,

            // Validation errors
            ImportExportError::ValidationError { .. } => ErrorCode::ImportColumnMappingInvalid,
            ImportExportError::MissingRequiredField(_) => ErrorCode::ImportColumnMappingInvalid,
            ImportExportError::InvalidFieldType { .. } => ErrorCode::ImportColumnMappingInvalid,

            // Mapping errors
            ImportExportError::MappingError { .. } => ErrorCode::ImportColumnMappingInvalid,
            ImportExportError::DuplicateRecord { .. } => ErrorCode::ImportPartialFailure,

            // Encryption errors
            ImportExportError::EncryptionFailed(_) => ErrorCode::CryptoEncryptionFailed,
            ImportExportError::KeyDerivationFailed(_) => ErrorCode::CryptoKeyDerivationFailed,

            // Session errors
            ImportExportError::SessionNotFound(_) => ErrorCode::ImportPartialFailure,
            ImportExportError::InvalidSessionStatus { .. } => ErrorCode::ImportPartialFailure,
            ImportExportError::SessionCancelled => ErrorCode::ImportPartialFailure,

            // Vault errors — opaque string, cannot delegate to specific variant
            ImportExportError::VaultError(_) => ErrorCode::ImportExportInternalError,

            // General errors
            ImportExportError::InternalError(_) => ErrorCode::ImportExportInternalError,
            ImportExportError::Timeout => ErrorCode::ImportExportTimeout,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        match self {
            ImportExportError::FileNotFound(path) => ErrorContext::new().file_path(path.clone()),
            ImportExportError::FileTooLarge { path, .. } => {
                ErrorContext::new().file_path(path.clone())
            }
            ImportExportError::FileReadError { path, .. } => {
                ErrorContext::new().file_path(path.clone())
            }
            ImportExportError::FileWriteError { path, .. } => {
                ErrorContext::new().file_path(path.clone())
            }
            ImportExportError::ValidationError { field, .. } => {
                ErrorContext::new().field_name(field)
            }
            ImportExportError::InvalidFieldType { field, .. } => {
                ErrorContext::new().field_name(field)
            }
            ImportExportError::MappingError { source_field, .. } => {
                ErrorContext::new().field_name(source_field)
            }
            ImportExportError::DuplicateRecord { name, .. } => {
                ErrorContext::new().record_name(name)
            }
            ImportExportError::SessionNotFound(id) => ErrorContext::new().record_id(*id),
            ImportExportError::InvalidSessionStatus { expected, actual } => ErrorContext::new()
                .field_name(expected)
                .extra("actual", actual),
            _ => ErrorContext::new(),
        }
    }

    fn to_fallback_message(&self) -> String {
        self.to_string()
    }
}

impl From<ImportExportError> for crate::errors::ServiceErrorBox {
    fn from(err: ImportExportError) -> Self {
        Box::new(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path() -> PathBuf {
        PathBuf::from("/tmp/test_import.csv")
    }

    #[test]
    fn file_not_found_returns_import_file_unreadable() {
        let err = ImportExportError::FileNotFound(test_path());
        assert_eq!(err.to_error_code(), ErrorCode::ImportFileUnreadable);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn decryption_failed_returns_import_password_incorrect() {
        let err = ImportExportError::DecryptionFailed("bad key".into());
        assert_eq!(err.to_error_code(), ErrorCode::ImportPasswordIncorrect);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn invalid_password_returns_import_password_incorrect() {
        let err = ImportExportError::InvalidPassword;
        assert_eq!(err.to_error_code(), ErrorCode::ImportPasswordIncorrect);
    }

    #[test]
    fn password_required_returns_import_password_required() {
        let err = ImportExportError::PasswordRequired;
        assert_eq!(err.to_error_code(), ErrorCode::ImportPasswordRequired);
    }

    #[test]
    fn encryption_failed_is_fatal() {
        assert_eq!(
            ImportExportError::EncryptionFailed("aes error".into()).to_error_code(),
            ErrorCode::CryptoEncryptionFailed
        );
        assert_eq!(
            ImportExportError::EncryptionFailed("aes error".into())
                .to_error_code()
                .level(),
            crate::errors::ErrorLevel::Fatal
        );
    }

    #[test]
    fn key_derivation_failed_is_crypto_error() {
        assert_eq!(
            ImportExportError::KeyDerivationFailed("argon2".into()).to_error_code(),
            ErrorCode::CryptoKeyDerivationFailed
        );
        assert_eq!(
            ImportExportError::KeyDerivationFailed("argon2".into())
                .to_error_code()
                .level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn internal_error_is_operation() {
        assert_eq!(
            ImportExportError::InternalError("bug".into()).to_error_code(),
            ErrorCode::ImportExportInternalError
        );
        assert_eq!(
            ImportExportError::InternalError("bug".into())
                .to_error_code()
                .level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn timeout_is_import_export_timeout() {
        assert_eq!(
            ImportExportError::Timeout.to_error_code(),
            ErrorCode::ImportExportTimeout
        );
        assert_eq!(
            ImportExportError::Timeout.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn duplicate_record_is_partial_failure() {
        assert_eq!(
            ImportExportError::DuplicateRecord {
                name: "gmail".into(),
                reason: "already exists".into(),
            }
            .to_error_code(),
            ErrorCode::ImportPartialFailure
        );
        assert_eq!(
            ImportExportError::DuplicateRecord {
                name: "gmail".into(),
                reason: "already exists".into(),
            }
            .to_error_code()
            .level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn session_cancelled_is_partial_failure() {
        assert_eq!(
            ImportExportError::SessionCancelled.to_error_code(),
            ErrorCode::ImportPartialFailure
        );
    }

    #[test]
    fn file_write_error_is_export_write_failed() {
        assert_eq!(
            ImportExportError::FileWriteError {
                path: test_path(),
                reason: "disk full".into(),
            }
            .to_error_code(),
            ErrorCode::ExportWriteFailed
        );
        assert_eq!(
            ImportExportError::FileWriteError {
                path: test_path(),
                reason: "disk full".into(),
            }
            .to_error_code()
            .level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn file_not_found_has_path_context() {
        let err = ImportExportError::FileNotFound(test_path());
        let ctx = err.to_error_context();
        assert_eq!(ctx.file_path, Some(test_path()));
    }

    #[test]
    fn file_too_large_has_all_fields_context() {
        let err = ImportExportError::FileTooLarge {
            path: test_path(),
            size: 5_000_000,
            max: 1_000_000,
        };
        let ctx = err.to_error_context();
        assert_eq!(ctx.file_path, Some(test_path()));
    }

    #[test]
    fn validation_error_has_field_context() {
        let err = ImportExportError::ValidationError {
            field: "username".into(),
            reason: "empty".into(),
        };
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("username".to_string()));
    }

    #[test]
    fn session_not_found_has_id_context() {
        let id = Uuid::new_v4();
        let err = ImportExportError::SessionNotFound(id);
        let ctx = err.to_error_context();
        assert_eq!(ctx.record_id, Some(id));
    }

    #[test]
    fn decryption_failed_has_no_context() {
        let err = ImportExportError::DecryptionFailed("bad".into());
        assert!(err.to_error_context().field_name.is_none());
    }

    #[test]
    fn import_export_error_converts_to_service_error_box() {
        let err = ImportExportError::FileNotFound(test_path());
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code(), ErrorCode::ImportFileUnreadable);
    }
}
