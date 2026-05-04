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
                // Heuristic: check if error is corruption-related
                let error_msg = e.to_string().to_lowercase();
                if matches!(e, rusqlite::Error::InvalidColumnType(_, _, _))
                    || error_msg.contains("corrupt")
                    || error_msg.contains("corrupted")
                    || error_msg.contains("malformed")
                {
                    ErrorCode::VaultDatabaseCorrupted
                } else {
                    ErrorCode::VaultDatabaseIoError
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
            VaultError::InvalidField { record_type: _, field } => {
                ErrorContext::new().field_name(format!("{:?}", field))
            }
        }
    }

    fn to_fallback_message(&self) -> String {
        match self {
            VaultError::RecordNotFound(_) => "The requested vault record was not found".to_string(),
            VaultError::VersionConflict { expected, actual } => {
                format!("Version conflict: expected version {}, but found version {}", expected, actual)
            }
            VaultError::TagAlreadyExists(name) => format!("Tag '{}' already exists", name),
            VaultError::TagNotFound(name) => format!("Tag '{}' not found", name),
            VaultError::NotUnlocked => "Vault is not unlocked. Please provide the master password".to_string(),
            VaultError::DatabaseError(e) => format!("Database error: {}", e),
            VaultError::CryptoError(msg) => format!("Cryptographic operation failed: {}", msg),
            VaultError::InvalidField { record_type, field } => {
                format!("Field '{:?}' is not valid for credential type '{:?}'", field, record_type)
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

    #[test]
    fn record_not_found_error_code_returns_vault_variant() {
        let id = Uuid::new_v4();
        let err = VaultError::RecordNotFound(id);
        let code = err.error_code();

        assert!(
            matches!(code, ErrorCode::Vault(ref msg) if msg.contains(&id.to_string())),
            "expected ErrorCode::Vault containing the UUID, got {:?}",
            code
        );
    }

    #[test]
    fn record_not_found_error_level_is_warning() {
        let err = VaultError::RecordNotFound(Uuid::new_v4());
        assert_eq!(err.error_level(), ErrorLevel::Warning);
    }

    #[test]
    fn not_unlocked_error_level_is_fatal() {
        let err = VaultError::NotUnlocked;
        assert_eq!(err.error_level(), ErrorLevel::Fatal);
    }

    #[test]
    fn version_conflict_display_contains_numbers() {
        let err = VaultError::VersionConflict {
            expected: 1,
            actual: 2,
        };
        let msg = err.to_string();
        assert!(
            msg.contains('1') && msg.contains('2'),
            "expected message to contain version numbers, got: {}",
            msg
        );
    }

    #[test]
    fn version_conflict_error_level_is_error() {
        let err = VaultError::VersionConflict {
            expected: 1,
            actual: 2,
        };
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn database_error_mapped_from_rusqlite() {
        let sqlite_err = rusqlite::Error::InvalidColumnIndex(99);
        let err: VaultError = sqlite_err.into();
        assert!(matches!(err, VaultError::DatabaseError(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn crypto_error_mapped_from_string() {
        let err: VaultError = "decryption failed".to_string().into();
        assert!(matches!(err, VaultError::CryptoError(ref s) if s == "decryption failed"));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn tag_errors_have_correct_levels() {
        assert_eq!(
            VaultError::TagAlreadyExists("work".into()).error_level(),
            ErrorLevel::Error
        );
        assert_eq!(
            VaultError::TagNotFound("missing".into()).error_level(),
            ErrorLevel::Error
        );
    }

    #[test]
    fn invalid_field_error_code_is_vault() {
        let err = VaultError::InvalidField {
            record_type: CredentialType::Login,
            field: FieldSelector::Password,
        };
        assert!(matches!(err.error_code(), ErrorCode::Vault(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn vault_error_converts_to_service_error_box() {
        let err = VaultError::NotUnlocked;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.error_level(), ErrorLevel::Fatal);
    }
}
