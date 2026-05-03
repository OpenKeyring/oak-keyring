use crate::db::DbError;
use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext};
use crate::types::credential::DataError;

impl ServiceError for DbError {
    fn to_error_code(&self) -> ErrorCode {
        match self {
            DbError::Sqlite(_) => ErrorCode::VaultDatabaseIoError,
            DbError::Data(data_err) => data_error_to_code(data_err),
            DbError::Uuid(_) => ErrorCode::DataInvalidUuid,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        match self {
            DbError::Sqlite(_) => ErrorContext::new(),
            DbError::Data(data_err) => data_error_to_context(data_err),
            DbError::Uuid(_) => ErrorContext::new(),
        }
    }

    fn to_fallback_message(&self) -> String {
        self.to_string()
    }
}

fn data_error_to_code(err: &DataError) -> ErrorCode {
    match err {
        DataError::FieldTooLong { .. } => ErrorCode::DataFieldTooLong,
        DataError::MissingField(_) => ErrorCode::DataMissingField,
        DataError::EmptyField(_) => ErrorCode::DataEmptyField,
        DataError::InvalidCredentialType(_) => ErrorCode::DataInvalidCredentialType,
        DataError::InvalidAuditOperation(_) => ErrorCode::DataInvalidAuditOperation,
        DataError::InvalidUuid(_) => ErrorCode::DataInvalidUuid,
        DataError::InvalidSyncStatus(_) => ErrorCode::DataInvalidCredentialType,
    }
}

fn data_error_to_context(err: &DataError) -> ErrorContext {
    match err {
        DataError::FieldTooLong { field, max, actual } => ErrorContext::new()
            .field_name(field.to_string())
            .expected_version(*max as u64)
            .actual_version(*actual as u64),
        DataError::MissingField(field) | DataError::EmptyField(field) => {
            ErrorContext::new().field_name(field.to_string())
        }
        DataError::InvalidCredentialType(t) => ErrorContext::new().field_name(t.to_string()),
        DataError::InvalidAuditOperation(op) => ErrorContext::new().field_name(op.to_string()),
        DataError::InvalidUuid(s) => ErrorContext::new().field_name(s.to_string()),
        DataError::InvalidSyncStatus(v) => ErrorContext::new().field_name(v.to_string()),
    }
}

impl From<DbError> for crate::errors::ServiceErrorBox {
    fn from(err: DbError) -> Self {
        Box::new(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_error_maps_to_vault_database_io_error() {
        let err = DbError::Sqlite(rusqlite::Error::InvalidColumnIndex(99));
        assert_eq!(err.to_error_code(), ErrorCode::VaultDatabaseIoError);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Fatal
        );
    }

    #[test]
    fn data_error_maps_to_minor_level() {
        let err = DbError::Data(DataError::MissingField("test"));
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
    }

    #[test]
    fn uuid_error_maps_to_minor_level() {
        let err = DbError::Uuid(uuid::Uuid::parse_str("not-a-uuid").unwrap_err());
        assert_eq!(err.to_error_code(), ErrorCode::DataInvalidUuid);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
    }

    #[test]
    fn sqlite_error_from_rusqlite() {
        let sqlite_err = rusqlite::Error::InvalidColumnIndex(42);
        let err: DbError = sqlite_err.into();
        assert!(matches!(err, DbError::Sqlite(_)));
        assert_eq!(err.to_error_code(), ErrorCode::VaultDatabaseIoError);
    }

    #[test]
    fn db_error_converts_to_service_error_box() {
        let err = DbError::Sqlite(rusqlite::Error::InvalidColumnIndex(1));
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code(), ErrorCode::VaultDatabaseIoError);
        assert_eq!(
            boxed.to_error_code().level(),
            crate::errors::ErrorLevel::Fatal
        );
    }

    #[test]
    fn sqlite_error_context_is_empty() {
        let err = DbError::Sqlite(rusqlite::Error::InvalidColumnIndex(1));
        let ctx = err.to_error_context();
        assert!(ctx.field_name.is_none());
    }

    #[test]
    fn uuid_error_context_is_empty() {
        let err = DbError::Uuid(uuid::Uuid::parse_str("bad").unwrap_err());
        let ctx = err.to_error_context();
        assert!(ctx.field_name.is_none());
    }

    #[test]
    fn data_error_field_too_long_has_context() {
        let err = DbError::Data(DataError::FieldTooLong {
            field: "title",
            max: 100,
            actual: 200,
        });
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("title".to_string()));
        assert_eq!(ctx.expected_version, Some(100));
        assert_eq!(ctx.actual_version, Some(200));
    }

    #[test]
    fn data_error_missing_field_has_context() {
        let err = DbError::Data(DataError::MissingField("username"));
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("username".to_string()));
    }

    #[test]
    fn data_error_empty_field_has_context() {
        let err = DbError::Data(DataError::EmptyField("password"));
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("password".to_string()));
    }

    #[test]
    fn data_error_invalid_credential_type_has_context() {
        let err = DbError::Data(DataError::InvalidCredentialType("unknown".into()));
        assert_eq!(err.to_error_code(), ErrorCode::DataInvalidCredentialType);
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("unknown".to_string()));
    }

    #[test]
    fn data_error_invalid_uuid_has_context() {
        let err = DbError::Data(DataError::InvalidUuid("not-a-uuid".into()));
        assert_eq!(err.to_error_code(), ErrorCode::DataInvalidUuid);
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("not-a-uuid".to_string()));
    }
}
