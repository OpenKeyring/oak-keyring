use crate::crypto::CryptoError;
use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorLevel};

impl ServiceError for CryptoError {
    fn error_code(&self) -> ErrorCode {
        ErrorCode::Crypto(self.to_string())
    }

    // CryptoError variants are opaque unit types — they carry no structured data.
    // Context (record_id, record_name) should be attached by the service layer
    // when wrapping CryptoError into a higher-level error type.
    fn error_context(&self) -> Option<crate::errors::ErrorContext> {
        None
    }

    fn error_level(&self) -> ErrorLevel {
        match self {
            CryptoError::EncryptionFailed => ErrorLevel::Fatal,
            CryptoError::DecryptionFailed => ErrorLevel::Error,
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
    fn encryption_failed_is_fatal() {
        let err = CryptoError::EncryptionFailed;
        assert!(matches!(err.error_code(), ErrorCode::Crypto(_)));
        assert_eq!(err.error_level(), ErrorLevel::Fatal);
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
    fn only_encryption_failed_is_fatal() {
        let non_fatal = [
            CryptoError::DecryptionFailed,
            CryptoError::InvalidKey,
            CryptoError::InvalidNonce,
            CryptoError::DerivationFailed,
        ];
        for v in &non_fatal {
            assert_eq!(v.error_level(), ErrorLevel::Error);
        }
        assert_eq!(
            CryptoError::EncryptionFailed.error_level(),
            ErrorLevel::Fatal
        );
    }

    #[test]
    fn crypto_error_converts_to_service_error_box() {
        let err = CryptoError::DecryptionFailed;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert!(matches!(boxed.error_code(), ErrorCode::Crypto(_)));
        assert_eq!(boxed.error_level(), ErrorLevel::Error);
    }

    #[test]
    fn encryption_failed_converts_to_fatal_service_error() {
        let err = CryptoError::EncryptionFailed;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert!(matches!(boxed.error_code(), ErrorCode::Crypto(_)));
        assert_eq!(boxed.error_level(), ErrorLevel::Fatal);
    }

    #[test]
    fn error_context_is_none() {
        let err = CryptoError::InvalidKey;
        assert!(err.error_context().is_none());
    }
}
