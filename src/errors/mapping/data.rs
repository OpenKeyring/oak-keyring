use crate::db::DbError;
use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext, ErrorLevel};
use crate::types::credential::DataError;

impl ServiceError for DbError {
    fn error_code(&self) -> ErrorCode {
        ErrorCode::Db(self.to_string())
    }

    fn error_context(&self) -> Option<ErrorContext> {
        match self {
            DbError::Data(data_err) => data_error_context(data_err),
            DbError::Sqlite(_) | DbError::Uuid(_) => None,
        }
    }

    fn error_level(&self) -> ErrorLevel {
        match self {
            DbError::Sqlite(_) => ErrorLevel::Error,
            DbError::Data(_) => ErrorLevel::Warning,
            DbError::Uuid(_) => ErrorLevel::Warning,
        }
    }
}

fn data_error_context(err: &DataError) -> Option<ErrorContext> {
    match err {
        DataError::FieldTooLong { field, max, actual } => Some(
            ErrorContext::new()
                .with("field_name", field)
                .with("max", &max.to_string())
                .with("actual", &actual.to_string()),
        ),
        DataError::MissingField(field) | DataError::EmptyField(field) => {
            Some(ErrorContext::new().with("field_name", field))
        }
        DataError::InvalidCredentialType(t) => {
            Some(ErrorContext::new().with("field_name", "credential_type").with("value", t))
        }
        DataError::InvalidAuditOperation(op) => {
            Some(ErrorContext::new().with("field_name", "audit_operation").with("value", op))
        }
        DataError::InvalidSyncStatus(v) => {
            Some(ErrorContext::new().with("field_name", "sync_status").with("value", &v.to_string()))
        }
        DataError::InvalidUuid(s) => {
            Some(ErrorContext::new().with("field_name", "uuid").with("value", s))
        }
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
    fn sqlite_error_maps_to_error_level() {
        let err = DbError::Sqlite(rusqlite::Error::InvalidColumnIndex(99));
        assert!(matches!(err.error_code(), ErrorCode::Db(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn data_error_maps_to_warning() {
        let err = DbError::Data(DataError::MissingField("test"));
        assert!(matches!(err.error_code(), ErrorCode::Db(_)));
        assert_eq!(err.error_level(), ErrorLevel::Warning);
    }

    #[test]
    fn uuid_error_maps_to_warning() {
        let err = DbError::Uuid(uuid::Uuid::parse_str("not-a-uuid").unwrap_err());
        assert!(matches!(err.error_code(), ErrorCode::Db(_)));
        assert_eq!(err.error_level(), ErrorLevel::Warning);
    }

    #[test]
    fn sqlite_error_from_rusqlite() {
        let sqlite_err = rusqlite::Error::InvalidColumnIndex(42);
        let err: DbError = sqlite_err.into();
        assert!(matches!(err, DbError::Sqlite(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn db_error_converts_to_service_error_box() {
        let err = DbError::Sqlite(rusqlite::Error::InvalidColumnIndex(1));
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert!(matches!(boxed.error_code(), ErrorCode::Db(_)));
        assert_eq!(boxed.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn sqlite_error_context_is_none() {
        let err = DbError::Sqlite(rusqlite::Error::InvalidColumnIndex(1));
        assert!(err.error_context().is_none());
    }

    #[test]
    fn uuid_error_context_is_none() {
        let err = DbError::Uuid(uuid::Uuid::parse_str("bad").unwrap_err());
        assert!(err.error_context().is_none());
    }

    #[test]
    fn data_error_field_too_long_has_context() {
        let err = DbError::Data(DataError::FieldTooLong {
            field: "title",
            max: 100,
            actual: 200,
        });
        let ctx = err.error_context().expect("expected context");
        assert_eq!(ctx.fields.get("field_name").unwrap(), "title");
        assert_eq!(ctx.fields.get("max").unwrap(), "100");
        assert_eq!(ctx.fields.get("actual").unwrap(), "200");
    }

    #[test]
    fn data_error_missing_field_has_context() {
        let err = DbError::Data(DataError::MissingField("username"));
        let ctx = err.error_context().expect("expected context");
        assert_eq!(ctx.fields.get("field_name").unwrap(), "username");
    }

    #[test]
    fn data_error_empty_field_has_context() {
        let err = DbError::Data(DataError::EmptyField("password"));
        let ctx = err.error_context().expect("expected context");
        assert_eq!(ctx.fields.get("field_name").unwrap(), "password");
    }

    #[test]
    fn data_error_invalid_credential_type_has_context() {
        let err = DbError::Data(DataError::InvalidCredentialType("unknown".into()));
        let ctx = err.error_context().expect("expected context");
        assert_eq!(ctx.fields.get("field_name").unwrap(), "credential_type");
        assert_eq!(ctx.fields.get("value").unwrap(), "unknown");
    }

    #[test]
    fn data_error_invalid_uuid_has_context() {
        let err = DbError::Data(DataError::InvalidUuid("not-a-uuid".into()));
        let ctx = err.error_context().expect("expected context");
        assert_eq!(ctx.fields.get("field_name").unwrap(), "uuid");
        assert_eq!(ctx.fields.get("value").unwrap(), "not-a-uuid");
    }
}
