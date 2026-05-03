use crate::errors::{ErrorCode, ErrorContext};

/// Trait for mapping domain errors to unified ErrorCode + ErrorContext.
///
/// Each Service module implements this trait for its error type.
/// Executor calls trait methods to get structured error info without
/// knowing the specific error variant.
pub trait ServiceError: std::error::Error + Send + Sync + 'static {
    /// Map this error to a specific ErrorCode variant.
    fn to_error_code(&self) -> ErrorCode;

    /// Extract structured context from this error.
    fn to_error_context(&self) -> ErrorContext;

    /// Generate a fallback message for when i18n fails.
    fn to_fallback_message(&self) -> String;
}

/// Type alias for boxed ServiceError.
pub type ServiceErrorBox = Box<dyn ServiceError>;
