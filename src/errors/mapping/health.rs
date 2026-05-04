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
            Self::VaultNotUnlocked => ErrorCode::ExecutorVaultLocked,
            Self::DecryptionFailed(_) => ErrorCode::CryptoDecryptionFailed,
            Self::HibpApiError(_) | Self::Internal(_) => ErrorCode::HealthHibpApiError,
            Self::HibpRateLimited => ErrorCode::HealthHibpRateLimited,
            Self::Disabled => ErrorCode::HealthCheckFailed,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        ErrorContext::new()
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
    use crate::errors::service_error::ServiceError;
    use crate::errors::{ErrorCode, ErrorLevel};

    #[test]
    fn vault_not_unlocked_maps_to_executor_vault_locked() {
        let err = HealthError::VaultNotUnlocked;
        assert_eq!(err.to_error_code(), ErrorCode::ExecutorVaultLocked);
        assert_eq!(err.error_level(), ErrorLevel::Operation);
        assert!(err.to_fallback_message().contains("not unlocked"));
    }

    #[test]
    fn decryption_failed_maps_to_crypto_decryption_failed() {
        let err = HealthError::DecryptionFailed("bad key".to_string());
        assert_eq!(err.to_error_code(), ErrorCode::CryptoDecryptionFailed);
        assert_eq!(err.error_level(), ErrorLevel::Operation);
        assert!(err.to_fallback_message().contains("decryption failed"));
    }

    #[test]
    fn hibp_api_error_maps_to_health_hibp_api_error() {
        let err = HealthError::HibpApiError("timeout".to_string());
        assert_eq!(err.to_error_code(), ErrorCode::HealthHibpApiError);
        assert_eq!(err.error_level(), ErrorLevel::Minor);
        assert!(err.to_fallback_message().contains("timeout"));
    }

    #[test]
    fn internal_error_maps_to_health_hibp_api_error() {
        let err = HealthError::Internal("test".to_string());
        assert_eq!(err.to_error_code(), ErrorCode::HealthHibpApiError);
        assert_eq!(err.error_level(), ErrorLevel::Minor);
    }

    #[test]
    fn hibp_rate_limited_maps_to_health_hibp_rate_limited() {
        let err = HealthError::HibpRateLimited;
        assert_eq!(err.to_error_code(), ErrorCode::HealthHibpRateLimited);
        assert_eq!(err.error_level(), ErrorLevel::Minor);
        assert!(err.to_fallback_message().contains("rate limited"));
    }

    #[test]
    fn disabled_maps_to_health_check_failed() {
        let err = HealthError::Disabled;
        assert_eq!(err.to_error_code(), ErrorCode::HealthCheckFailed);
        assert_eq!(err.error_level(), ErrorLevel::Minor);
        assert!(err.to_fallback_message().contains("disabled"));
    }

    #[test]
    fn health_error_converts_to_service_error_box() {
        let err = HealthError::HibpRateLimited;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code(), ErrorCode::HealthHibpRateLimited);
    }

    #[test]
    fn health_error_implements_service_error() {
        fn assert_service_error<E: ServiceError>(_: &E) {}
        assert_service_error(&HealthError::VaultNotUnlocked);
        assert_service_error(&HealthError::DecryptionFailed("x".into()));
        assert_service_error(&HealthError::HibpApiError("y".into()));
        assert_service_error(&HealthError::Internal("z".into()));
        assert_service_error(&HealthError::HibpRateLimited);
        assert_service_error(&HealthError::Disabled);
    }
}
