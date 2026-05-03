use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext};

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
    fn to_error_code(&self) -> ErrorCode {
        match self {
            ClipboardError::AccessDenied => ErrorCode::ClipboardUnavailable,
            ClipboardError::ContentTooLong { .. } => ErrorCode::ClipboardCopyFailed,
            ClipboardError::ContentMismatch => ErrorCode::ClipboardClearFailed,
            ClipboardError::PlatformUnavailable(_) => ErrorCode::ClipboardUnavailable,
            ClipboardError::LockPoisoned => ErrorCode::ClipboardUnavailable,
            ClipboardError::Io(_) => ErrorCode::ClipboardCopyFailed,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        match self {
            ClipboardError::PlatformUnavailable(reason) => ErrorContext::new().field_name(reason),
            ClipboardError::ContentTooLong {
                max_bytes,
                actual_bytes,
            } => ErrorContext::new()
                .expected_version(*max_bytes as u64)
                .actual_version(*actual_bytes as u64),
            _ => ErrorContext::new(),
        }
    }

    fn to_fallback_message(&self) -> String {
        self.to_string()
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

    #[test]
    fn clipboard_error_access_denied() {
        let err = ClipboardError::AccessDenied;
        assert_eq!(err.to_error_code(), ErrorCode::ClipboardUnavailable);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
    }

    #[test]
    fn clipboard_error_content_too_long() {
        let err = ClipboardError::ContentTooLong {
            max_bytes: 1024,
            actual_bytes: 2048,
        };
        assert_eq!(err.to_error_code(), ErrorCode::ClipboardCopyFailed);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
        assert!(err.to_fallback_message().contains("1024"));
        assert!(err.to_fallback_message().contains("2048"));
    }

    #[test]
    fn clipboard_error_platform_unavailable() {
        let err = ClipboardError::PlatformUnavailable("headless".to_string());
        assert_eq!(err.to_error_code(), ErrorCode::ClipboardUnavailable);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
        assert!(err.to_fallback_message().contains("headless"));
        let ctx = err.to_error_context();
        assert_eq!(ctx.field_name, Some("headless".to_string()));
    }

    #[test]
    fn clipboard_error_content_mismatch_is_clear_failed() {
        let err = ClipboardError::ContentMismatch;
        assert_eq!(err.to_error_code(), ErrorCode::ClipboardClearFailed);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
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
