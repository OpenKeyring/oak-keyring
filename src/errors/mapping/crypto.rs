use crate::crypto::CryptoError;
use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorLevel};

impl ServiceError for CryptoError {
    fn error_code(&self) -> ErrorCode {
        ErrorCode::Crypto(self.to_string())
    }

    fn error_context(&self) -> Option<crate::errors::ErrorContext> {
        None
    }

    fn error_level(&self) -> ErrorLevel {
        match self {
            CryptoError::DecryptionFailed => ErrorLevel::Error,
            CryptoError::EncryptionFailed => ErrorLevel::Error,
            CryptoError::InvalidKey => ErrorLevel::Error,
            CryptoError::InvalidNonce => ErrorLevel::Error,
            CryptoError::DerivationFailed => ErrorLevel::Error,
        }
    }
}

impl From<CryptoError> for crate::errors::ServiceErrorBox {
    fn from(err: CryptoError) -> Self {
        Box::new(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decryption_failed_error_code_is_crypto() {
        let err = CryptoError::DecryptionFailed;
        let code = err.error_code();
        assert!(
            matches!(code, ErrorCode::Crypto(ref msg) if msg.contains("decryption")),
            "expected ErrorCode::Crypto containing 'decryption', got {:?}",
            code
        );
    }

    #[test]
    fn encryption_failed_error_code_is_crypto() {
        let err = CryptoError::EncryptionFailed;
        assert!(matches!(err.error_code(), ErrorCode::Crypto(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn invalid_key_error_code_is_crypto() {
        let err = CryptoError::InvalidKey;
        assert!(matches!(err.error_code(), ErrorCode::Crypto(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn invalid_nonce_error_code_is_crypto() {
        let err = CryptoError::InvalidNonce;
        assert!(matches!(err.error_code(), ErrorCode::Crypto(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn derivation_failed_error_code_is_crypto() {
        let err = CryptoError::DerivationFailed;
        assert!(matches!(err.error_code(), ErrorCode::Crypto(_)));
        assert_eq!(err.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn all_variants_have_error_level() {
        let variants = [
            CryptoError::DecryptionFailed,
            CryptoError::EncryptionFailed,
            CryptoError::InvalidKey,
            CryptoError::InvalidNonce,
            CryptoError::DerivationFailed,
        ];
        for v in &variants {
            assert_eq!(v.error_level(), ErrorLevel::Error);
        }
    }

    #[test]
    fn crypto_error_converts_to_service_error_box() {
        let err = CryptoError::DecryptionFailed;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert!(matches!(boxed.error_code(), ErrorCode::Crypto(_)));
        assert_eq!(boxed.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn error_context_is_none() {
        let err = CryptoError::InvalidKey;
        assert!(err.error_context().is_none());
    }
}
