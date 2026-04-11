use uuid::Uuid;

use crate::commands::types::FieldSelector;
use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext, ErrorLevel};
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
    fn error_code(&self) -> ErrorCode {
        ErrorCode::Vault(self.to_string())
    }

    fn error_context(&self) -> Option<ErrorContext> {
        None
    }

    fn error_level(&self) -> ErrorLevel {
        match self {
            VaultError::NotUnlocked => ErrorLevel::Fatal,
            VaultError::RecordNotFound(_) => ErrorLevel::Warning,
            VaultError::VersionConflict { .. } => ErrorLevel::Error,
            VaultError::TagAlreadyExists(_) => ErrorLevel::Error,
            VaultError::TagNotFound(_) => ErrorLevel::Error,
            VaultError::DatabaseError(_) => ErrorLevel::Error,
            VaultError::CryptoError(_) => ErrorLevel::Error,
            VaultError::InvalidField { .. } => ErrorLevel::Error,
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
