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

impl From<ImportExportError> for crate::errors::ServiceErrorBox {
    fn from(err: ImportExportError) -> Self {
        Box::new(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a dummy path for tests
    fn test_path() -> PathBuf {
        PathBuf::from("/tmp/test_import.csv")
    }

    // -- error_code tests --

    #[test]
    fn file_not_found_returns_import_export_code() {
        let err = ImportExportError::FileNotFound(test_path());
        assert!(
            matches!(err.error_code(), ErrorCode::ImportExport(ref msg) if msg.contains("test_import.csv")),
            "expected ErrorCode::ImportExport containing the filename"
        );
    }

    #[test]
    fn decryption_failed_returns_import_export_code() {
        let err = ImportExportError::DecryptionFailed("bad key".into());
        assert!(
            matches!(err.error_code(), ErrorCode::ImportExport(ref msg) if msg.contains("bad key"))
        );
    }

    #[test]
    fn timeout_returns_import_export_code() {
        let err = ImportExportError::Timeout;
        assert!(matches!(err.error_code(), ErrorCode::ImportExport(_)));
    }

    #[test]
    fn parse_error_returns_import_export_code() {
        let err = ImportExportError::ParseError {
            format: "CSV".into(),
            reason: "unexpected EOF".into(),
        };
        assert!(
            matches!(err.error_code(), ErrorCode::ImportExport(ref msg) if msg.contains("CSV"))
        );
    }

    #[test]
    fn session_not_found_returns_import_export_code() {
        let id = Uuid::new_v4();
        let err = ImportExportError::SessionNotFound(id);
        assert!(
            matches!(err.error_code(), ErrorCode::ImportExport(ref msg) if msg.contains(&id.to_string()))
        );
    }

    // -- error_level tests --

    #[test]
    fn fatal_level_for_encryption_failed() {
        assert_eq!(
            ImportExportError::EncryptionFailed("aes error".into()).error_level(),
            ErrorLevel::Fatal
        );
    }

    #[test]
    fn fatal_level_for_key_derivation_failed() {
        assert_eq!(
            ImportExportError::KeyDerivationFailed("argon2".into()).error_level(),
            ErrorLevel::Fatal
        );
    }

    #[test]
    fn fatal_level_for_internal_error() {
        assert_eq!(
            ImportExportError::InternalError("bug".into()).error_level(),
            ErrorLevel::Fatal
        );
    }

    #[test]
    fn warning_level_for_duplicate_record() {
        assert_eq!(
            ImportExportError::DuplicateRecord {
                name: "gmail".into(),
                reason: "already exists".into(),
            }
            .error_level(),
            ErrorLevel::Warning
        );
    }

    #[test]
    fn warning_level_for_session_cancelled() {
        assert_eq!(
            ImportExportError::SessionCancelled.error_level(),
            ErrorLevel::Warning
        );
    }

    #[test]
    fn error_level_for_file_not_found() {
        assert_eq!(
            ImportExportError::FileNotFound(test_path()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_file_too_large() {
        assert_eq!(
            ImportExportError::FileTooLarge {
                path: test_path(),
                size: 5_000_000,
                max: 1_000_000,
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_file_read_error() {
        assert_eq!(
            ImportExportError::FileReadError {
                path: test_path(),
                reason: "permission denied".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_file_write_error() {
        assert_eq!(
            ImportExportError::FileWriteError {
                path: test_path(),
                reason: "disk full".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_decryption_failed() {
        assert_eq!(
            ImportExportError::DecryptionFailed("corrupt".into()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_invalid_password() {
        assert_eq!(
            ImportExportError::InvalidPassword.error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_password_required() {
        assert_eq!(
            ImportExportError::PasswordRequired.error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_parse_error() {
        assert_eq!(
            ImportExportError::ParseError {
                format: "JSON".into(),
                reason: "invalid syntax".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_invalid_format() {
        assert_eq!(
            ImportExportError::InvalidFormat("broken".into()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_unsupported_format() {
        assert_eq!(
            ImportExportError::UnsupportedFormat("XML".into()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_validation_error() {
        assert_eq!(
            ImportExportError::ValidationError {
                field: "username".into(),
                reason: "empty".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_missing_required_field() {
        assert_eq!(
            ImportExportError::MissingRequiredField("title".into()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_invalid_field_type() {
        assert_eq!(
            ImportExportError::InvalidFieldType {
                field: "port".into(),
                expected: "number".into(),
                actual: "string".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_mapping_error() {
        assert_eq!(
            ImportExportError::MappingError {
                source_field: "login".into(),
                reason: "type mismatch".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_session_not_found() {
        assert_eq!(
            ImportExportError::SessionNotFound(Uuid::new_v4()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_invalid_session_status() {
        assert_eq!(
            ImportExportError::InvalidSessionStatus {
                expected: "Active".into(),
                actual: "Completed".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_vault_error() {
        assert_eq!(
            ImportExportError::VaultError("locked".into()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_timeout() {
        assert_eq!(ImportExportError::Timeout.error_level(), ErrorLevel::Error);
    }

    // -- Display / error message tests --

    #[test]
    fn file_not_found_display_message() {
        let err = ImportExportError::FileNotFound(test_path());
        let msg = err.to_string();
        assert!(msg.contains("file not found"), "got: {msg}");
        assert!(msg.contains("test_import.csv"), "got: {msg}");
    }

    #[test]
    fn file_too_large_display_message() {
        let err = ImportExportError::FileTooLarge {
            path: test_path(),
            size: 5_000_000,
            max: 1_000_000,
        };
        let msg = err.to_string();
        assert!(msg.contains("file too large"), "got: {msg}");
        assert!(msg.contains("5000000"), "got: {msg}");
        assert!(msg.contains("1000000"), "got: {msg}");
    }

    #[test]
    fn invalid_password_display_message() {
        let err = ImportExportError::InvalidPassword;
        assert_eq!(err.to_string(), "invalid password");
    }

    #[test]
    fn session_cancelled_display_message() {
        let err = ImportExportError::SessionCancelled;
        assert_eq!(err.to_string(), "session was cancelled");
    }

    #[test]
    fn timeout_display_message() {
        let err = ImportExportError::Timeout;
        assert_eq!(err.to_string(), "operation timed out");
    }

    #[test]
    fn invalid_field_type_display_message() {
        let err = ImportExportError::InvalidFieldType {
            field: "port".into(),
            expected: "number".into(),
            actual: "string".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("port"), "got: {msg}");
        assert!(msg.contains("number"), "got: {msg}");
        assert!(msg.contains("string"), "got: {msg}");
    }

    #[test]
    fn duplicate_record_display_message() {
        let err = ImportExportError::DuplicateRecord {
            name: "gmail".into(),
            reason: "already exists".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("gmail"), "got: {msg}");
        assert!(msg.contains("already exists"), "got: {msg}");
    }

    // -- error_context tests --

    #[test]
    fn file_not_found_has_path_context() {
        let err = ImportExportError::FileNotFound(test_path());
        let ctx = err.error_context().expect("should have context");
        assert_eq!(ctx.fields.get("path").unwrap(), "/tmp/test_import.csv");
    }

    #[test]
    fn file_too_large_has_all_fields_context() {
        let err = ImportExportError::FileTooLarge {
            path: test_path(),
            size: 5_000_000,
            max: 1_000_000,
        };
        let ctx = err.error_context().expect("should have context");
        assert_eq!(ctx.fields.get("path").unwrap(), "/tmp/test_import.csv");
        assert_eq!(ctx.fields.get("size").unwrap(), "5000000");
        assert_eq!(ctx.fields.get("max").unwrap(), "1000000");
    }

    #[test]
    fn validation_error_has_field_context() {
        let err = ImportExportError::ValidationError {
            field: "username".into(),
            reason: "empty".into(),
        };
        let ctx = err.error_context().expect("should have context");
        assert_eq!(ctx.fields.get("field").unwrap(), "username");
    }

    #[test]
    fn session_not_found_has_id_context() {
        let id = Uuid::new_v4();
        let err = ImportExportError::SessionNotFound(id);
        let ctx = err.error_context().expect("should have context");
        assert_eq!(ctx.fields.get("session_id").unwrap(), &id.to_string());
    }

    #[test]
    fn decryption_failed_has_no_context() {
        let err = ImportExportError::DecryptionFailed("bad".into());
        assert!(err.error_context().is_none());
    }

    #[test]
    fn invalid_password_has_no_context() {
        assert!(ImportExportError::InvalidPassword.error_context().is_none());
    }

    #[test]
    fn timeout_has_no_context() {
        assert!(ImportExportError::Timeout.error_context().is_none());
    }

    // -- From<ImportExportError> for ServiceErrorBox --

    #[test]
    fn import_export_error_converts_to_service_error_box() {
        let err = ImportExportError::FileNotFound(test_path());
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn fatal_error_converts_and_preserves_level() {
        let err = ImportExportError::EncryptionFailed("key error".into());
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.error_level(), ErrorLevel::Fatal);
    }

    #[test]
    fn warning_error_converts_and_preserves_level() {
        let err = ImportExportError::SessionCancelled;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.error_level(), ErrorLevel::Warning);
    }
}
impl ServiceError for ImportExportError {
    fn to_error_code(&self) -> ErrorCode {
        match self {
            // File read errors → ImportFileUnreadable
            ImportExportError::FileNotFound(_) => ErrorCode::ImportFileUnreadable,
            ImportExportError::FileTooLarge { .. } => ErrorCode::ImportFileUnreadable,
            ImportExportError::FileReadError { .. } => ErrorCode::ImportFileUnreadable,

            // Parse/format errors → ImportFileFormatInvalid
            ImportExportError::ParseError { .. } => ErrorCode::ImportFileFormatInvalid,
            ImportExportError::InvalidFormat(_) => ErrorCode::ImportFileFormatInvalid,
            ImportExportError::UnsupportedFormat(_) => ErrorCode::ImportFileFormatInvalid,

            // Password errors
            ImportExportError::PasswordRequired => ErrorCode::ImportPasswordRequired,
            ImportExportError::InvalidPassword => ErrorCode::ImportPasswordIncorrect,
            ImportExportError::DecryptionFailed(_) => ErrorCode::ImportPasswordIncorrect,

            // Column mapping errors → ImportColumnMappingInvalid
            ImportExportError::MissingRequiredField(_) => ErrorCode::ImportColumnMappingInvalid,
            ImportExportError::InvalidFieldType { .. } => ErrorCode::ImportColumnMappingInvalid,
            ImportExportError::ValidationError { .. } => ErrorCode::ImportColumnMappingInvalid,
            ImportExportError::MappingError { .. } => ErrorCode::ImportColumnMappingInvalid,

            // Partial failures → ImportPartialFailure
            ImportExportError::DuplicateRecord { .. } => ErrorCode::ImportPartialFailure,
            ImportExportError::SessionNotFound(_) => ErrorCode::ImportPartialFailure,
            ImportExportError::InvalidSessionStatus { .. } => ErrorCode::ImportPartialFailure,
            ImportExportError::SessionCancelled => ErrorCode::ImportPartialFailure,

            // Export errors
            ImportExportError::FileWriteError { .. } => ErrorCode::ExportWriteFailed,
            ImportExportError::EncryptionFailed(_) => ErrorCode::ExportWriteFailed,
            ImportExportError::KeyDerivationFailed(_) => ErrorCode::ExportWriteFailed,

            // Other errors → ExportPathInvalid (general export failure)
            ImportExportError::InternalError(_) => ErrorCode::ExportPathInvalid,
            ImportExportError::Timeout => ErrorCode::ExportPathInvalid,
            ImportExportError::VaultError(_) => ErrorCode::ExportPathInvalid,
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
            ImportExportError::MissingRequiredField(field) => {
                ErrorContext::new().field_name(field.clone())
            }
            ImportExportError::InvalidFieldType { field, .. } => {
                ErrorContext::new().field_name(field.clone())
            }
            ImportExportError::ValidationError { field, .. } => {
                ErrorContext::new().field_name(field.clone())
            }
            ImportExportError::MappingError { source_field, .. } => {
                ErrorContext::new().field_name(source_field.clone())
            }
            ImportExportError::DuplicateRecord { name, .. } => {
                ErrorContext::new().record_name(name.clone())
            }
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

    // Helper to create a dummy path for tests
    fn test_path() -> PathBuf {
        PathBuf::from("/tmp/test_import.csv")
    }

    // -- error_code tests --

    #[test]
    fn file_not_found_returns_import_export_code() {
        let err = ImportExportError::FileNotFound(test_path());
        assert!(
            matches!(err.error_code(), ErrorCode::ImportExport(ref msg) if msg.contains("test_import.csv")),
            "expected ErrorCode::ImportExport containing the filename"
        );
    }

    #[test]
    fn decryption_failed_returns_import_export_code() {
        let err = ImportExportError::DecryptionFailed("bad key".into());
        assert!(
            matches!(err.error_code(), ErrorCode::ImportExport(ref msg) if msg.contains("bad key"))
        );
    }

    #[test]
    fn timeout_returns_import_export_code() {
        let err = ImportExportError::Timeout;
        assert!(matches!(err.error_code(), ErrorCode::ImportExport(_)));
    }

    #[test]
    fn parse_error_returns_import_export_code() {
        let err = ImportExportError::ParseError {
            format: "CSV".into(),
            reason: "unexpected EOF".into(),
        };
        assert!(
            matches!(err.error_code(), ErrorCode::ImportExport(ref msg) if msg.contains("CSV"))
        );
    }

    #[test]
    fn session_not_found_returns_import_export_code() {
        let id = Uuid::new_v4();
        let err = ImportExportError::SessionNotFound(id);
        assert!(
            matches!(err.error_code(), ErrorCode::ImportExport(ref msg) if msg.contains(&id.to_string()))
        );
    }

    // -- error_level tests --

    #[test]
    fn fatal_level_for_encryption_failed() {
        assert_eq!(
            ImportExportError::EncryptionFailed("aes error".into()).error_level(),
            ErrorLevel::Fatal
        );
    }

    #[test]
    fn fatal_level_for_key_derivation_failed() {
        assert_eq!(
            ImportExportError::KeyDerivationFailed("argon2".into()).error_level(),
            ErrorLevel::Fatal
        );
    }

    #[test]
    fn fatal_level_for_internal_error() {
        assert_eq!(
            ImportExportError::InternalError("bug".into()).error_level(),
            ErrorLevel::Fatal
        );
    }

    #[test]
    fn warning_level_for_duplicate_record() {
        assert_eq!(
            ImportExportError::DuplicateRecord {
                name: "gmail".into(),
                reason: "already exists".into(),
            }
            .error_level(),
            ErrorLevel::Warning
        );
    }

    #[test]
    fn warning_level_for_session_cancelled() {
        assert_eq!(
            ImportExportError::SessionCancelled.error_level(),
            ErrorLevel::Warning
        );
    }

    #[test]
    fn error_level_for_file_not_found() {
        assert_eq!(
            ImportExportError::FileNotFound(test_path()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_file_too_large() {
        assert_eq!(
            ImportExportError::FileTooLarge {
                path: test_path(),
                size: 5_000_000,
                max: 1_000_000,
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_file_read_error() {
        assert_eq!(
            ImportExportError::FileReadError {
                path: test_path(),
                reason: "permission denied".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_file_write_error() {
        assert_eq!(
            ImportExportError::FileWriteError {
                path: test_path(),
                reason: "disk full".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_decryption_failed() {
        assert_eq!(
            ImportExportError::DecryptionFailed("corrupt".into()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_invalid_password() {
        assert_eq!(
            ImportExportError::InvalidPassword.error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_password_required() {
        assert_eq!(
            ImportExportError::PasswordRequired.error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_parse_error() {
        assert_eq!(
            ImportExportError::ParseError {
                format: "JSON".into(),
                reason: "invalid syntax".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_invalid_format() {
        assert_eq!(
            ImportExportError::InvalidFormat("broken".into()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_unsupported_format() {
        assert_eq!(
            ImportExportError::UnsupportedFormat("XML".into()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_validation_error() {
        assert_eq!(
            ImportExportError::ValidationError {
                field: "username".into(),
                reason: "empty".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_missing_required_field() {
        assert_eq!(
            ImportExportError::MissingRequiredField("title".into()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_invalid_field_type() {
        assert_eq!(
            ImportExportError::InvalidFieldType {
                field: "port".into(),
                expected: "number".into(),
                actual: "string".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_mapping_error() {
        assert_eq!(
            ImportExportError::MappingError {
                source_field: "login".into(),
                reason: "type mismatch".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_session_not_found() {
        assert_eq!(
            ImportExportError::SessionNotFound(Uuid::new_v4()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_invalid_session_status() {
        assert_eq!(
            ImportExportError::InvalidSessionStatus {
                expected: "Active".into(),
                actual: "Completed".into(),
            }
            .error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_vault_error() {
        assert_eq!(
            ImportExportError::VaultError("locked".into()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn error_level_for_timeout() {
        assert_eq!(ImportExportError::Timeout.error_level(), ErrorLevel::Error);
    }

    // -- Display / error message tests --

    #[test]
    fn file_not_found_display_message() {
        let err = ImportExportError::FileNotFound(test_path());
        let msg = err.to_string();
        assert!(msg.contains("file not found"), "got: {msg}");
        assert!(msg.contains("test_import.csv"), "got: {msg}");
    }

    #[test]
    fn file_too_large_display_message() {
        let err = ImportExportError::FileTooLarge {
            path: test_path(),
            size: 5_000_000,
            max: 1_000_000,
        };
        let msg = err.to_string();
        assert!(msg.contains("file too large"), "got: {msg}");
        assert!(msg.contains("5000000"), "got: {msg}");
        assert!(msg.contains("1000000"), "got: {msg}");
    }

    #[test]
    fn invalid_password_display_message() {
        let err = ImportExportError::InvalidPassword;
        assert_eq!(err.to_string(), "invalid password");
    }

    #[test]
    fn session_cancelled_display_message() {
        let err = ImportExportError::SessionCancelled;
        assert_eq!(err.to_string(), "session was cancelled");
    }

    #[test]
    fn timeout_display_message() {
        let err = ImportExportError::Timeout;
        assert_eq!(err.to_string(), "operation timed out");
    }

    #[test]
    fn invalid_field_type_display_message() {
        let err = ImportExportError::InvalidFieldType {
            field: "port".into(),
            expected: "number".into(),
            actual: "string".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("port"), "got: {msg}");
        assert!(msg.contains("number"), "got: {msg}");
        assert!(msg.contains("string"), "got: {msg}");
    }

    #[test]
    fn duplicate_record_display_message() {
        let err = ImportExportError::DuplicateRecord {
            name: "gmail".into(),
            reason: "already exists".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("gmail"), "got: {msg}");
        assert!(msg.contains("already exists"), "got: {msg}");
    }

    // -- error_context tests --

    #[test]
    fn file_not_found_has_path_context() {
        let err = ImportExportError::FileNotFound(test_path());
        let ctx = err.error_context().expect("should have context");
        assert_eq!(ctx.fields.get("path").unwrap(), "/tmp/test_import.csv");
    }

    #[test]
    fn file_too_large_has_all_fields_context() {
        let err = ImportExportError::FileTooLarge {
            path: test_path(),
            size: 5_000_000,
            max: 1_000_000,
        };
        let ctx = err.error_context().expect("should have context");
        assert_eq!(ctx.fields.get("path").unwrap(), "/tmp/test_import.csv");
        assert_eq!(ctx.fields.get("size").unwrap(), "5000000");
        assert_eq!(ctx.fields.get("max").unwrap(), "1000000");
    }

    #[test]
    fn validation_error_has_field_context() {
        let err = ImportExportError::ValidationError {
            field: "username".into(),
            reason: "empty".into(),
        };
        let ctx = err.error_context().expect("should have context");
        assert_eq!(ctx.fields.get("field").unwrap(), "username");
    }

    #[test]
    fn session_not_found_has_id_context() {
        let id = Uuid::new_v4();
        let err = ImportExportError::SessionNotFound(id);
        let ctx = err.error_context().expect("should have context");
        assert_eq!(ctx.fields.get("session_id").unwrap(), &id.to_string());
    }

    #[test]
    fn decryption_failed_has_no_context() {
        let err = ImportExportError::DecryptionFailed("bad".into());
        assert!(err.error_context().is_none());
    }

    #[test]
    fn invalid_password_has_no_context() {
        assert!(ImportExportError::InvalidPassword.error_context().is_none());
    }

    #[test]
    fn timeout_has_no_context() {
        assert!(ImportExportError::Timeout.error_context().is_none());
    }

    // -- From<ImportExportError> for ServiceErrorBox --

    #[test]
    fn import_export_error_converts_to_service_error_box() {
        let err = ImportExportError::FileNotFound(test_path());
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn fatal_error_converts_and_preserves_level() {
        let err = ImportExportError::EncryptionFailed("key error".into());
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.error_level(), ErrorLevel::Fatal);
    }

    #[test]
    fn warning_error_converts_and_preserves_level() {
        let err = ImportExportError::SessionCancelled;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.error_level(), ErrorLevel::Warning);
    }
}
