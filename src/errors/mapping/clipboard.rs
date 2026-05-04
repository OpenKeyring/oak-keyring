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
            Self::AccessDenied | Self::PlatformUnavailable { .. } => {
                ErrorCode::ClipboardUnavailable
            }
            Self::ContentTooLong { .. }
            | Self::ContentMismatch
            | Self::LockPoisoned
            | Self::Io(_) => ErrorCode::ClipboardCopyFailed,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        match self {
            Self::ContentTooLong {
                max_bytes,
                actual_bytes,
            } => ErrorContext::new()
                .record_name(format!("{} bytes", actual_bytes))
                .attempt_count(*max_bytes as u32),
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
    use crate::errors::service_error::ServiceError;
    use crate::errors::{ErrorCode, ErrorLevel};

    #[test]
    fn clipboard_error_access_denied() {
        let err = ClipboardError::AccessDenied;
        assert_eq!(err.to_error_code(), ErrorCode::ClipboardUnavailable);
        assert_eq!(err.error_level(), ErrorLevel::Minor);
        assert!(err.to_fallback_message().contains("access denied"));
    }

    #[test]
    fn clipboard_error_content_too_long() {
        let err = ClipboardError::ContentTooLong {
            max_bytes: 1024,
            actual_bytes: 2048,
        };
        assert_eq!(err.to_error_code(), ErrorCode::ClipboardCopyFailed);
        assert_eq!(err.error_level(), ErrorLevel::Minor);
        assert!(err.to_string().contains("1024"));
        assert!(err.to_string().contains("2048"));
        let ctx = err.to_error_context();
        assert_eq!(ctx.attempt_count, Some(1024));
    }

    #[test]
    fn clipboard_error_platform_unavailable() {
        let err = ClipboardError::PlatformUnavailable("headless".to_string());
        assert_eq!(err.to_error_code(), ErrorCode::ClipboardUnavailable);
        assert_eq!(err.error_level(), ErrorLevel::Minor);
        assert!(err.to_string().contains("headless"));
    }

    #[test]
    fn clipboard_error_content_mismatch() {
        let err = ClipboardError::ContentMismatch;
        assert_eq!(err.to_error_code(), ErrorCode::ClipboardCopyFailed);
        assert_eq!(err.error_level(), ErrorLevel::Minor);
        assert!(err.to_fallback_message().contains("mismatch"));
    }

    #[test]
    fn clipboard_error_lock_poisoned() {
        let err = ClipboardError::LockPoisoned;
        assert_eq!(err.to_error_code(), ErrorCode::ClipboardCopyFailed);
        assert_eq!(err.error_level(), ErrorLevel::Minor);
    }

    #[test]
    fn clipboard_error_io() {
        let err = ClipboardError::Io("permission denied".to_string());
        assert_eq!(err.to_error_code(), ErrorCode::ClipboardCopyFailed);
        assert_eq!(err.error_level(), ErrorLevel::Minor);
        assert!(err.to_fallback_message().contains("permission denied"));
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
