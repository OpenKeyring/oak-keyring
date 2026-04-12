use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext, ErrorLevel};

#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    #[error("vault not unlocked")]
    VaultNotUnlocked,

    #[error("record decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("HIBP API error: {0}")]
    HibpApiError(String),

    #[error("HIBP rate limited")]
    HibpRateLimited,

    #[error("health check disabled")]
    Disabled,

    #[error("internal error: {0}")]
    Internal(String),
}

impl ServiceError for HealthError {
    fn error_code(&self) -> ErrorCode {
        ErrorCode::Health(self.to_string())
    }

    fn error_context(&self) -> Option<ErrorContext> {
        None
    }

    fn error_level(&self) -> ErrorLevel {
        match self {
            HealthError::VaultNotUnlocked => ErrorLevel::Fatal,
            HealthError::Disabled => ErrorLevel::Warning,
            HealthError::HibpApiError(_) => ErrorLevel::Warning,
            HealthError::HibpRateLimited => ErrorLevel::Warning,
            HealthError::DecryptionFailed(_) => ErrorLevel::Error,
            HealthError::Internal(_) => ErrorLevel::Error,
        }
    }
}

impl From<HealthError> for crate::errors::ServiceErrorBox {
    fn from(err: HealthError) -> Self {
        Box::new(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_not_unlocked_error_level_is_fatal() {
        let err = HealthError::VaultNotUnlocked;
        assert_eq!(err.error_level(), ErrorLevel::Fatal);
    }

    #[test]
    fn hibp_api_error_level_is_warning() {
        let err = HealthError::HibpApiError("timeout".into());
        assert_eq!(err.error_level(), ErrorLevel::Warning);
    }

    #[test]
    fn disabled_error_level_is_warning() {
        let err = HealthError::Disabled;
        assert_eq!(err.error_level(), ErrorLevel::Warning);
    }

    #[test]
    fn decryption_failed_error_level_is_error() {
        let err = HealthError::DecryptionFailed("bad key".into());
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn health_error_converts_to_service_error_box() {
        let err = HealthError::HibpRateLimited;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.error_level(), ErrorLevel::Warning);
    }

    #[test]
    fn health_error_code_is_health_variant() {
        let err = HealthError::Internal("test".into());
        assert!(matches!(err.error_code(), ErrorCode::Health(_)));
    }
}
