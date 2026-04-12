use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext, ErrorLevel};

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("Clipboard access denied")]
    AccessDenied,

    #[error("Content too long: {actual_bytes} bytes (max {max_bytes})")]
    ContentTooLong {
        max_bytes: usize,
        actual_bytes: usize,
    },

    #[error("Clipboard content changed since copy — skipping clear")]
    ContentMismatch,

    #[error("Clipboard unavailable: {0}")]
    PlatformUnavailable(String),

    #[error("Clipboard lock poisoned")]
    LockPoisoned,

    #[error("Clipboard I/O error: {0}")]
    Io(String),
}

impl ServiceError for ClipboardError {
    fn error_code(&self) -> ErrorCode {
        ErrorCode::Clipboard(self.to_string())
    }

    fn error_context(&self) -> Option<ErrorContext> {
        match self {
            Self::PlatformUnavailable(reason) => Some(ErrorContext::new().with("reason", reason)),
            Self::ContentTooLong {
                max_bytes,
                actual_bytes,
            } => Some(
                ErrorContext::new()
                    .with("max_bytes", &max_bytes.to_string())
                    .with("actual_bytes", &actual_bytes.to_string()),
            ),
            _ => None,
        }
    }

    fn error_level(&self) -> ErrorLevel {
        match self {
            Self::ContentMismatch => ErrorLevel::Warning,
            _ => ErrorLevel::Error,
        }
    }
}

impl From<ClipboardError> for crate::errors::ServiceErrorBox {
    fn from(err: ClipboardError) -> Self {
        Box::new(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::service_error::ServiceError;
    use crate::errors::{ErrorCode, ErrorLevel};

    #[test]
    fn clipboard_error_access_denied() {
        let err = ClipboardError::AccessDenied;
        assert!(matches!(err.error_code(), ErrorCode::Clipboard(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn clipboard_error_content_too_long() {
        let err = ClipboardError::ContentTooLong {
            max_bytes: 1024,
            actual_bytes: 2048,
        };
        assert!(matches!(err.error_code(), ErrorCode::Clipboard(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.to_string().contains("1024"));
        assert!(err.to_string().contains("2048"));
    }

    #[test]
    fn clipboard_error_platform_unavailable() {
        let err = ClipboardError::PlatformUnavailable("headless".to_string());
        assert!(matches!(err.error_code(), ErrorCode::Clipboard(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
        assert!(err.to_string().contains("headless"));
        assert!(err.error_context().is_some());
    }

    #[test]
    fn clipboard_error_content_mismatch_is_warning() {
        let err = ClipboardError::ContentMismatch;
        assert!(matches!(err.error_code(), ErrorCode::Clipboard(_)));
        assert_eq!(err.error_level(), ErrorLevel::Warning);
    }

    #[test]
    fn clipboard_error_implements_service_error() {
        fn assert_service_error<E: ServiceError>(_: &E) {}
        assert_service_error(&ClipboardError::AccessDenied);
        assert_service_error(&ClipboardError::LockPoisoned);
        assert_service_error(&ClipboardError::Io("test".into()));
        assert_service_error(&ClipboardError::PlatformUnavailable("x".into()));
        assert_service_error(&ClipboardError::ContentTooLong {
            max_bytes: 0,
            actual_bytes: 0,
        });
        assert_service_error(&ClipboardError::ContentMismatch);
    }
}
