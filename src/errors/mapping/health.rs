use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext};

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
    fn to_error_code(&self) -> ErrorCode {
        match self {
            HealthError::VaultNotUnlocked => ErrorCode::ExecutorVaultLocked,
            HealthError::DecryptionFailed(_) => ErrorCode::CryptoDecryptionFailed,
            HealthError::HibpApiError(_) => ErrorCode::HealthHibpApiError,
            HealthError::HibpRateLimited => ErrorCode::HealthHibpRateLimited,
            HealthError::Disabled => ErrorCode::HealthCheckFailed,
            HealthError::Internal(_) => ErrorCode::HealthCheckFailed,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        match self {
            HealthError::DecryptionFailed(name) => ErrorContext::new().record_name(name),
            _ => ErrorContext::new(),
        }
    }

    fn to_fallback_message(&self) -> String {
        self.to_string()
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
    fn vault_not_unlocked_error_code_is_executor_vault_locked() {
        let err = HealthError::VaultNotUnlocked;
        assert_eq!(err.to_error_code(), ErrorCode::ExecutorVaultLocked);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn hibp_api_error_is_minor() {
        let err = HealthError::HibpApiError("timeout".into());
        assert_eq!(err.to_error_code(), ErrorCode::HealthHibpApiError);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
    }

    #[test]
    fn hibp_rate_limited_is_minor() {
        let err = HealthError::HibpRateLimited;
        assert_eq!(err.to_error_code(), ErrorCode::HealthHibpRateLimited);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
    }

    #[test]
    fn disabled_is_minor() {
        let err = HealthError::Disabled;
        assert_eq!(err.to_error_code(), ErrorCode::HealthCheckFailed);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
    }

    #[test]
    fn decryption_failed_is_operation() {
        let err = HealthError::DecryptionFailed("bad key".into());
        assert_eq!(err.to_error_code(), ErrorCode::CryptoDecryptionFailed);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
        let ctx = err.to_error_context();
        assert_eq!(ctx.record_name, Some("bad key".to_string()));
    }

    #[test]
    fn health_error_converts_to_service_error_box() {
        let err = HealthError::HibpRateLimited;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code(), ErrorCode::HealthHibpRateLimited);
        assert_eq!(
            boxed.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
    }

    #[test]
    fn internal_error_is_minor() {
        let err = HealthError::Internal("test".into());
        assert_eq!(err.to_error_code(), ErrorCode::HealthCheckFailed);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Minor
        );
    }
}
