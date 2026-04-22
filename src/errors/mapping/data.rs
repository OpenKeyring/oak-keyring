use crate::db::DbError;
use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorLevel};

impl ServiceError for DbError {
    fn error_code(&self) -> ErrorCode {
        ErrorCode::Db(self.to_string())
    }

    fn error_context(&self) -> Option<crate::errors::ErrorContext> {
        None
    }

    fn error_level(&self) -> ErrorLevel {
        match self {
            DbError::Sqlite(_) => ErrorLevel::Fatal,
            DbError::Data(_) => ErrorLevel::Error,
            DbError::Uuid(_) => ErrorLevel::Error,
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
    fn sqlite_error_maps_to_fatal() {
        let err = DbError::Sqlite(rusqlite::Error::InvalidColumnIndex(99));
        assert!(matches!(err.error_code(), ErrorCode::Db(_)));
        assert_eq!(err.error_level(), ErrorLevel::Fatal);
    }

    #[test]
    fn data_error_maps_to_error_level() {
        let err = DbError::Data(crate::types::credential::DataError::MissingField("test"));
        assert!(matches!(err.error_code(), ErrorCode::Db(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn uuid_error_maps_to_error_level() {
        let err = DbError::Uuid(uuid::Uuid::parse_str("not-a-uuid").unwrap_err());
        assert!(matches!(err.error_code(), ErrorCode::Db(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn sqlite_error_from_rusqlite() {
        let sqlite_err = rusqlite::Error::InvalidColumnIndex(42);
        let err: DbError = sqlite_err.into();
        assert!(matches!(err, DbError::Sqlite(_)));
        assert_eq!(err.error_level(), ErrorLevel::Fatal);
    }

    #[test]
    fn db_error_converts_to_service_error_box() {
        let err = DbError::Sqlite(rusqlite::Error::InvalidColumnIndex(1));
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert!(matches!(boxed.error_code(), ErrorCode::Db(_)));
        assert_eq!(boxed.error_level(), ErrorLevel::Fatal);
    }

    #[test]
    fn error_context_is_none() {
        let err = DbError::Sqlite(rusqlite::Error::InvalidColumnIndex(1));
        assert!(err.error_context().is_none());
    }
}
