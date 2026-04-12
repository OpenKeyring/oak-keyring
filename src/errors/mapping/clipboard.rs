use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext, ErrorLevel};

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("platform clipboard unavailable: {0}")]
    PlatformUnavailable(String),

    #[error("clipboard lock poisoned")]
    LockPoisoned,

    #[error("clipboard I/O error: {0}")]
    Io(String),
}

impl ServiceError for ClipboardError {
    fn error_code(&self) -> ErrorCode {
        ErrorCode::Clipboard(self.to_string())
    }

    fn error_context(&self) -> Option<ErrorContext> {
        None
    }

    fn error_level(&self) -> ErrorLevel {
        match self {
            ClipboardError::PlatformUnavailable(_) => ErrorLevel::Fatal,
            ClipboardError::LockPoisoned => ErrorLevel::Error,
            ClipboardError::Io(_) => ErrorLevel::Error,
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

    #[test]
    fn platform_unavailable_error_level_is_fatal() {
        let err = ClipboardError::PlatformUnavailable("X11 not available".into());
        assert_eq!(err.error_level(), ErrorLevel::Fatal);
    }

    #[test]
    fn lock_poisoned_error_level_is_error() {
        let err = ClipboardError::LockPoisoned;
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn io_error_error_level_is_error() {
        let err = ClipboardError::Io("read failed".into());
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn clipboard_error_converts_to_service_error_box() {
        let err = ClipboardError::PlatformUnavailable("test".into());
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.error_level(), ErrorLevel::Fatal);
    }
}
