use uuid::Uuid;

use crate::commands::types::FieldSelector;
use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext};
use crate::types::credential::CredentialType;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("record not found: {0}")]
    RecordNotFound(Uuid),

    #[error("version conflict: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },

    #[error("tag already exists: {0}")]
    TagAlreadyExists(String),

    #[error("tag not found: {0}")]
    TagNotFound(String),

    #[error("vault is not unlocked")]
    NotUnlocked,

    #[error("database error: {0}")]
    DatabaseError(#[from] rusqlite::Error),

    #[error("crypto error: {0}")]
    CryptoError(String),

    #[error("invalid field {field:?} for credential type {record_type:?}")]
    InvalidField {
        record_type: CredentialType,
        field: FieldSelector,
    },
}

impl From<String> for VaultError {
    fn from(msg: String) -> Self {
        VaultError::CryptoError(msg)
    }
}

impl ServiceError for VaultError {
    fn to_error_code(&self) -> ErrorCode {
        match self {
            VaultError::RecordNotFound(_) => ErrorCode::VaultRecordNotFound,
            VaultError::VersionConflict { .. } => ErrorCode::VaultVersionConflict,
            VaultError::TagAlreadyExists(_) => ErrorCode::VaultTagAlreadyExists,
            VaultError::TagNotFound(_) => ErrorCode::VaultTagNotFound,
            VaultError::NotUnlocked => ErrorCode::VaultNotUnlocked,
            VaultError::DatabaseError(e) => {
                // Distinguish between corruption and I/O errors
                match e {
                    rusqlite::Error::InvalidPath(_) | rusqlite::Error::SqliteFailure(_, _) => {
                        ErrorCode::VaultDatabaseCorrupted
                    }
                    _ => ErrorCode::VaultDatabaseIoError,
                }
            }
            VaultError::CryptoError(_) => ErrorCode::CryptoDecryptionFailed,
            VaultError::InvalidField { .. } => ErrorCode::VaultInvalidField,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        match self {
            VaultError::RecordNotFound(id) => ErrorContext::new().record_id(*id),
            VaultError::VersionConflict { expected, actual } => ErrorContext::new()
                .expected_version(*expected)
                .actual_version(*actual),
            VaultError::TagAlreadyExists(name) => ErrorContext::new().field_name(name.clone()),
            VaultError::TagNotFound(name) => ErrorContext::new().field_name(name.clone()),
            VaultError::NotUnlocked => ErrorContext::new(),
            VaultError::DatabaseError(_) => ErrorContext::new(),
            VaultError::CryptoError(_) => ErrorContext::new(),
            VaultError::InvalidField { field, .. } => {
                ErrorContext::new().field_name(format!("{:?}", field))
            }
        }
    }

    fn to_fallback_message(&self) -> String {
        match self {
            VaultError::RecordNotFound(id) => format!("Record not found: {}", id),
            VaultError::VersionConflict { expected, actual } => {
                format!("Version conflict: expected {}, actual {}", expected, actual)
            }
            VaultError::TagAlreadyExists(name) => format!("Tag '{}' already exists", name),
            VaultError::TagNotFound(name) => format!("Tag '{}' not found", name),
            VaultError::NotUnlocked => "Vault is locked".to_string(),
            VaultError::DatabaseError(e) => format!("Database error: {}", e),
            VaultError::CryptoError(msg) => format!("Crypto error: {}", msg),
            VaultError::InvalidField { field, record_type } => {
                format!(
                    "Invalid field {:?} for credential type {:?}",
                    field, record_type
                )
            }
        }
    }
}

impl From<VaultError> for crate::errors::ServiceErrorBox {
    fn from(err: VaultError) -> Self {
        Box::new(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::ErrorLevel;

    #[test]
    fn record_not_found_error_code_is_specific() {
        let id = Uuid::new_v4();
        let err = VaultError::RecordNotFound(id);
        assert_eq!(err.to_error_code(), ErrorCode::VaultRecordNotFound);
    }

    #[test]
    fn record_not_found_error_level_is_operation() {
        let err = VaultError::RecordNotFound(Uuid::new_v4());
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn version_conflict_error_code_is_specific() {
        let err = VaultError::VersionConflict {
            expected: 1,
            actual: 2,
        };
        assert_eq!(err.to_error_code(), ErrorCode::VaultVersionConflict);
        let ctx = err.to_error_context();
        assert_eq!(ctx.expected_version, Some(1));
        assert_eq!(ctx.actual_version, Some(2));
    }

    #[test]
    fn version_conflict_error_level_is_operation() {
        let err = VaultError::VersionConflict {
            expected: 1,
            actual: 2,
        };
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn tag_already_exists_is_minor() {
        let err = VaultError::TagAlreadyExists("work".into());
        assert_eq!(err.to_error_code(), ErrorCode::VaultTagAlreadyExists);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn tag_not_found_is_minor() {
        let err = VaultError::TagNotFound("missing".into());
        assert_eq!(err.to_error_code(), ErrorCode::VaultTagNotFound);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn not_unlocked_is_operation() {
        let err = VaultError::NotUnlocked;
        assert_eq!(err.to_error_code(), ErrorCode::VaultNotUnlocked);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn database_corruption_is_fatal() {
        let sqlite_err = rusqlite::Error::InvalidPath(std::path::PathBuf::from("/bad"));
        let err: VaultError = sqlite_err.into();
        assert_eq!(err.to_error_code(), ErrorCode::VaultDatabaseCorrupted);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Fatal);
    }

    #[test]
    fn crypto_error_maps_to_decryption_failed() {
        let err: VaultError = "decryption failed".to_string().into();
        assert_eq!(err.to_error_code(), ErrorCode::CryptoDecryptionFailed);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn invalid_field_is_operation() {
        let err = VaultError::InvalidField {
            record_type: CredentialType::Login,
            field: FieldSelector::Password,
        };
        assert_eq!(err.to_error_code(), ErrorCode::VaultInvalidField);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn vault_error_converts_to_service_error_box() {
        let err = VaultError::NotUnlocked;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code(), ErrorCode::VaultNotUnlocked);
    }

    #[test]
    fn fallback_messages_are_human_readable() {
        assert_eq!(
            VaultError::NotUnlocked.to_fallback_message(),
            "Vault is locked"
        );
        assert!(VaultError::VersionConflict {
            expected: 1,
            actual: 2
        }
        .to_fallback_message()
        .contains("1"));
    }
}
