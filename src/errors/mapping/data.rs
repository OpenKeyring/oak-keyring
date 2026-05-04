use crate::db::DbError;
use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext};
use crate::types::credential::DataError;

impl ServiceError for DbError {
    fn to_error_code(&self) -> ErrorCode {
        match self {
            DbError::Sqlite(e) => {
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
            DbError::Data(data_err) => match data_err {
                DataError::FieldTooLong { .. } => ErrorCode::DataFieldTooLong,
                DataError::MissingField(_) => ErrorCode::DataMissingField,
                DataError::EmptyField(_) => ErrorCode::DataEmptyField,
                DataError::InvalidCredentialType(_) => ErrorCode::DataInvalidCredentialType,
                DataError::InvalidAuditOperation(_) => ErrorCode::DataInvalidAuditOperation,
                DataError::InvalidSyncStatus(_) => ErrorCode::DataInvalidCredentialType,
                DataError::InvalidUuid(_) => ErrorCode::DataInvalidUuid,
            },
            DbError::Uuid(_) => ErrorCode::DataInvalidUuid,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        match self {
            DbError::Sqlite(_) => ErrorContext::new(),
            DbError::Data(data_err) => match data_err {
                DataError::FieldTooLong { field, .. } => ErrorContext::new().field_name(*field),
                DataError::MissingField(field) => ErrorContext::new().field_name(*field),
                DataError::EmptyField(field) => ErrorContext::new().field_name(*field),
                DataError::InvalidCredentialType(_) => {
                    ErrorContext::new().field_name("credential_type")
                }
                DataError::InvalidAuditOperation(_) => {
                    ErrorContext::new().field_name("audit_operation")
                }
                DataError::InvalidSyncStatus(_) => ErrorContext::new().field_name("sync_status"),
                DataError::InvalidUuid(_) => ErrorContext::new().field_name("uuid"),
            },
            DbError::Uuid(_) => ErrorContext::new(),
        }
    }

    fn to_fallback_message(&self) -> String {
        match self {
            DbError::Sqlite(e) => format!("Database error: {}", e),
            DbError::Data(data_err) => match data_err {
                DataError::FieldTooLong { field, max, actual } => {
                    format!(
                        "Field '{}' is too long: maximum length is {}, but got {}",
                        field, max, actual
                    )
                }
                DataError::MissingField(field) => {
                    format!("Required field '{}' is missing", field)
                }
                DataError::EmptyField(field) => {
                    format!("Field '{}' cannot be empty", field)
                }
                DataError::InvalidCredentialType(t) => {
                    format!("Invalid credential type: '{}'", t)
                }
                DataError::InvalidAuditOperation(op) => {
                    format!("Invalid audit operation: '{}'", op)
                }
                DataError::InvalidSyncStatus(v) => {
                    format!("Invalid sync status value: {}", v)
                }
                DataError::InvalidUuid(s) => {
                    format!("Invalid UUID format: '{}'", s)
                }
            },
            DbError::Uuid(e) => format!("Invalid UUID: {}", e),
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
    use crate::errors::ErrorLevel;

    #[test]
    fn sqlite_error_corruption_detected() {
        let sqlite_err = rusqlite::Error::InvalidColumnType(
            0, // column index
            "INTEGER".to_string(),
            rusqlite::types::Type::Integer,
        );
        let err = DbError::Sqlite(sqlite_err);
        assert_eq!(err.to_error_code(), ErrorCode::VaultDatabaseCorrupted);
    }

    #[test]
    fn sqlite_error_io_error() {
        let err = DbError::Sqlite(rusqlite::Error::SqliteSingleThreadedMode);
        assert_eq!(err.to_error_code(), ErrorCode::VaultDatabaseIoError);
    }

    #[test]
    fn sqlite_error_has_empty_context() {
        let err = DbError::Sqlite(rusqlite::Error::InvalidColumnIndex(1));
        let ctx = err.to_error_context();
        assert!(ctx.record_id.is_none());
        assert!(ctx.field_name.is_none());
    }

    #[test]
    fn sqlite_error_level_is_fatal() {
        let err = DbError::Sqlite(rusqlite::Error::InvalidColumnIndex(1));
        assert_eq!(err.to_error_code().level(), ErrorLevel::Fatal);
    }

    #[test]
    fn data_error_field_too_long_maps_to_correct_code() {
        let err = DbError::Data(DataError::FieldTooLong {
            field: "title",
            max: 100,
            actual: 200,
        });
        assert_eq!(err.to_error_code(), ErrorCode::DataFieldTooLong);
    }

    #[test]
    fn data_error_field_too_long_has_field_name_context() {
        let err = DbError::Data(DataError::FieldTooLong {
            field: "title",
            max: 100,
            actual: 200,
        });
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("title".to_string()));
    }

    #[test]
    fn data_error_field_too_long_error_level_is_minor() {
        let err = DbError::Data(DataError::FieldTooLong {
            field: "title",
            max: 100,
            actual: 200,
        });
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn data_error_missing_field_maps_to_correct_code() {
        let err = DbError::Data(DataError::MissingField("username"));
        assert_eq!(err.to_error_code(), ErrorCode::DataMissingField);
    }

    #[test]
    fn data_error_missing_field_has_field_name_context() {
        let err = DbError::Data(DataError::MissingField("username"));
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("username".to_string()));
    }

    #[test]
    fn data_error_missing_field_error_level_is_minor() {
        let err = DbError::Data(DataError::MissingField("username"));
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn data_error_empty_field_maps_to_correct_code() {
        let err = DbError::Data(DataError::EmptyField("password"));
        assert_eq!(err.to_error_code(), ErrorCode::DataEmptyField);
    }

    #[test]
    fn data_error_empty_field_has_field_name_context() {
        let err = DbError::Data(DataError::EmptyField("password"));
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("password".to_string()));
    }

    #[test]
    fn data_error_empty_field_error_level_is_minor() {
        let err = DbError::Data(DataError::EmptyField("password"));
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn data_error_invalid_credential_type_maps_to_correct_code() {
        let err = DbError::Data(DataError::InvalidCredentialType("unknown".into()));
        assert_eq!(err.to_error_code(), ErrorCode::DataInvalidCredentialType);
    }

    #[test]
    fn data_error_invalid_credential_type_has_credential_type_context() {
        let err = DbError::Data(DataError::InvalidCredentialType("unknown".into()));
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("credential_type".to_string()));
    }

    #[test]
    fn data_error_invalid_audit_operation_maps_to_correct_code() {
        let err = DbError::Data(DataError::InvalidAuditOperation("delete".into()));
        assert_eq!(err.to_error_code(), ErrorCode::DataInvalidAuditOperation);
    }

    #[test]
    fn data_error_invalid_audit_operation_has_audit_operation_context() {
        let err = DbError::Data(DataError::InvalidAuditOperation("delete".into()));
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("audit_operation".to_string()));
    }

    #[test]
    fn data_error_invalid_sync_status_maps_to_invalid_credential_type() {
        let err = DbError::Data(DataError::InvalidSyncStatus(999));
        assert_eq!(err.to_error_code(), ErrorCode::DataInvalidCredentialType);
    }

    #[test]
    fn data_error_invalid_sync_status_has_sync_status_context() {
        let err = DbError::Data(DataError::InvalidSyncStatus(999));
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("sync_status".to_string()));
    }

    #[test]
    fn data_error_invalid_uuid_maps_to_correct_code() {
        let err = DbError::Data(DataError::InvalidUuid("not-a-uuid".into()));
        assert_eq!(err.to_error_code(), ErrorCode::DataInvalidUuid);
    }

    #[test]
    fn data_error_invalid_uuid_has_uuid_context() {
        let err = DbError::Data(DataError::InvalidUuid("not-a-uuid".into()));
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("uuid".to_string()));
    }

    #[test]
    fn uuid_error_maps_to_correct_code() {
        let err = DbError::Uuid(uuid::Uuid::parse_str("not-a-uuid").unwrap_err());
        assert_eq!(err.to_error_code(), ErrorCode::DataInvalidUuid);
    }

    #[test]
    fn uuid_error_has_empty_context() {
        let err = DbError::Uuid(uuid::Uuid::parse_str("bad").unwrap_err());
        let ctx = err.to_error_context();
        assert!(ctx.record_id.is_none());
        assert!(ctx.field_name.is_none());
    }

    #[test]
    fn uuid_error_level_is_minor() {
        let err = DbError::Uuid(uuid::Uuid::parse_str("bad").unwrap_err());
        assert_eq!(err.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn sqlite_error_from_rusqlite() {
        let sqlite_err = rusqlite::Error::InvalidColumnIndex(42);
        let err: DbError = sqlite_err.into();
        assert!(matches!(err, DbError::Sqlite(_)));
        assert_eq!(err.to_error_code().level(), ErrorLevel::Fatal);
    }

    #[test]
    fn db_error_converts_to_service_error_box() {
        let err = DbError::Data(DataError::MissingField("test"));
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code(), ErrorCode::DataMissingField);
        assert_eq!(boxed.to_error_code().level(), ErrorLevel::Minor);
    }

    #[test]
    fn fallback_messages_are_descriptive() {
        let test_cases = [
            (
                DbError::Data(DataError::FieldTooLong {
                    field: "title",
                    max: 100,
                    actual: 200,
                }),
                "too long",
            ),
            (
                DbError::Data(DataError::MissingField("username")),
                "missing",
            ),
            (DbError::Data(DataError::EmptyField("password")), "empty"),
            (
                DbError::Data(DataError::InvalidCredentialType("unknown".into())),
                "credential type",
            ),
            (DbError::Data(DataError::InvalidUuid("bad".into())), "uuid"),
        ];

        for (err, keyword) in test_cases {
            let msg = err.to_fallback_message();
            assert!(
                msg.to_lowercase().contains(keyword),
                "Expected fallback message to contain '{}', got: {}",
                keyword,
                msg
            );
        }
    }

    #[test]
    fn field_too_long_fallback_message_contains_details() {
        let err = DbError::Data(DataError::FieldTooLong {
            field: "title",
            max: 100,
            actual: 200,
        });
        let msg = err.to_fallback_message();
        assert!(msg.contains("title"));
        assert!(msg.contains("100"));
        assert!(msg.contains("200"));
    }

    #[test]
    fn invalid_credential_type_fallback_message_contains_value() {
        let err = DbError::Data(DataError::InvalidCredentialType("unknown".into()));
        let msg = err.to_fallback_message();
        assert!(msg.contains("unknown"));
    }

    #[test]
    fn invalid_sync_status_fallback_message_contains_value() {
        let err = DbError::Data(DataError::InvalidSyncStatus(999));
        let msg = err.to_fallback_message();
        assert!(msg.contains("999"));
    }
}
