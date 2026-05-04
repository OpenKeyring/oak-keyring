use crate::errors::{ErrorCode, ErrorContext};

/// Trait for service-specific errors that can be displayed to users.
///
/// All service errors should implement this trait to provide consistent error
/// information across the application. The trait provides:
/// - Error code for i18n message lookup
/// - Structured context for message interpolation
/// - Derived error level (via ErrorCode::level())
///
/// # Example
///
/// ```rust
/// use oak_keyring::errors::{ServiceError, ErrorCode, ErrorContext};
/// use thiserror::Error;
///
/// #[derive(Debug, Error)]
/// pub enum MyServiceError {
///     #[error("Record not found")]
///     NotFound { id: String },
/// }
///
/// impl ServiceError for MyServiceError {
///     fn to_error_code(&self) -> ErrorCode {
///         match self {
///             MyServiceError::NotFound { .. } => ErrorCode::VaultRecordNotFound,
///         }
///     }
///
///     fn to_error_context(&self) -> ErrorContext {
///         match self {
///             MyServiceError::NotFound { id } => ErrorContext::new()
///                 .record_name(id.clone()),
///         }
///     }
///
///     fn to_fallback_message(&self) -> String {
///         match self {
///             MyServiceError::NotFound { id } => {
///                 format!("Record '{}' not found", id)
///             }
///         }
///     }
/// }
/// ```
pub trait ServiceError: std::error::Error + Send + Sync + 'static {
    /// Returns the error code for this error.
    ///
    /// The error code is used to:
    /// - Retrieve localized error messages via i18n keys
    /// - Determine error level (Fatal/Operation/Minor)
    /// - Identify the error category (module prefix)
    ///
    /// # Returns
    ///
    /// An `ErrorCode` variant representing the specific error condition.
    fn to_error_code(&self) -> ErrorCode;

    /// Returns structured context for message interpolation.
    ///
    /// The context provides key-value pairs that can be interpolated into
    /// error messages to provide detailed, user-relevant information.
    ///
    /// # Returns
    ///
    /// An `ErrorContext` struct with fields relevant to this error.
    fn to_error_context(&self) -> ErrorContext;

    /// Returns a fallback English message for this error.
    ///
    /// This message is used when i18n lookup fails or is not available.
    /// It should provide a clear, user-friendly description of the error.
    ///
    /// # Returns
    ///
    /// A fallback error message in English.
    fn to_fallback_message(&self) -> String;

    /// Returns the error level for this error.
    ///
    /// This is derived from `self.to_error_code().level()` and should not
    /// be overridden in most cases.
    ///
    /// # Returns
    ///
    /// The `ErrorLevel` (Fatal/Operation/Minor) for this error.
    fn error_level(&self) -> crate::errors::ErrorLevel {
        self.to_error_code().level()
    }
}

/// Boxed service error trait object for type-erased error handling.
///
/// This type alias is used throughout the application to represent errors
/// that can be returned from any service.
pub type ServiceErrorBox = Box<dyn ServiceError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::ErrorLevel;

    // Mock error implementation for testing
    #[derive(Debug, thiserror::Error)]
    enum MockError {
        #[error("Record not found: {id}")]
        NotFound { id: String },
        #[error("Invalid field: {field}")]
        InvalidField { field: String },
    }

    impl ServiceError for MockError {
        fn to_error_code(&self) -> ErrorCode {
            match self {
                MockError::NotFound { .. } => ErrorCode::VaultRecordNotFound,
                MockError::InvalidField { .. } => ErrorCode::VaultInvalidField,
            }
        }

        fn to_error_context(&self) -> ErrorContext {
            match self {
                MockError::NotFound { id } => ErrorContext::new().record_name(id.clone()),
                MockError::InvalidField { field } => ErrorContext::new().field_name(field.clone()),
            }
        }

        fn to_fallback_message(&self) -> String {
            match self {
                MockError::NotFound { id } => format!("Record '{}' not found", id),
                MockError::InvalidField { field } => format!("Invalid field: {}", field),
            }
        }
    }

    #[test]
    fn service_error_returns_correct_code() {
        let error = MockError::NotFound {
            id: "test-record".to_string(),
        };
        assert_eq!(error.to_error_code(), ErrorCode::VaultRecordNotFound);
    }

    #[test]
    fn service_error_returns_context() {
        let error = MockError::NotFound {
            id: "test-record".to_string(),
        };
        let ctx = error.to_error_context();
        assert_eq!(ctx.record_name, Some("test-record".to_string()));
    }

    #[test]
    fn service_error_returns_fallback_message() {
        let error = MockError::NotFound {
            id: "my-record".to_string(),
        };
        let msg = error.to_fallback_message();
        assert!(msg.contains("my-record"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn service_error_derives_level_from_code() {
        let not_found = MockError::NotFound {
            id: "test".to_string(),
        };
        assert_eq!(not_found.error_level(), ErrorLevel::Operation);

        let invalid = MockError::InvalidField {
            field: "email".to_string(),
        };
        assert_eq!(invalid.error_level(), ErrorLevel::Operation);
    }

    #[test]
    fn service_error_is_send_sync() {
        // Ensure ServiceError trait object is Send + Sync
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ServiceErrorBox>();
    }

    #[test]
    fn service_error_box_can_be_created() {
        let error: ServiceErrorBox = Box::new(MockError::NotFound {
            id: "test".to_string(),
        });
        assert_eq!(error.to_error_code(), ErrorCode::VaultRecordNotFound);
    }
}
