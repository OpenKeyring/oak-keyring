use crate::crypto::CryptoError;
use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext};

impl ServiceError for CryptoError {
    fn to_error_code(&self) -> ErrorCode {
        match self {
            CryptoError::EncryptionFailed => ErrorCode::CryptoEncryptionFailed,
            CryptoError::DecryptionFailed => ErrorCode::CryptoDecryptionFailed,
            CryptoError::InvalidKey => ErrorCode::CryptoInvalidKey,
            CryptoError::InvalidNonce => ErrorCode::CryptoInvalidNonce,
            CryptoError::DerivationFailed => ErrorCode::CryptoKeyDerivationFailed,
        }
    }

    // CryptoError variants are opaque unit types — they carry no structured data.
    // Context (record_id, record_name) should be attached by the service layer
    // when wrapping CryptoError into a higher-level error type.
    fn to_error_context(&self) -> ErrorContext {
        ErrorContext::new()
    }

    fn to_fallback_message(&self) -> String {
        match self {
            CryptoError::EncryptionFailed => "Encryption failed",
            CryptoError::DecryptionFailed => "Decryption failed",
            CryptoError::InvalidKey => "Invalid key",
            CryptoError::InvalidNonce => "Invalid nonce",
            CryptoError::DerivationFailed => "Key derivation failed",
        }
        .to_string()
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
    fn decryption_failed_error_code_is_specific() {
        let err = CryptoError::DecryptionFailed;
        assert_eq!(err.to_error_code(), ErrorCode::CryptoDecryptionFailed);
    }

    #[test]
    fn encryption_failed_is_fatal() {
        let err = CryptoError::EncryptionFailed;
        assert_eq!(err.to_error_code(), ErrorCode::CryptoEncryptionFailed);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Fatal
        );
    }

    #[test]
    fn invalid_key_error_code_is_specific() {
        let err = CryptoError::InvalidKey;
        assert_eq!(err.to_error_code(), ErrorCode::CryptoInvalidKey);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn invalid_nonce_error_code_is_specific() {
        let err = CryptoError::InvalidNonce;
        assert_eq!(err.to_error_code(), ErrorCode::CryptoInvalidNonce);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn derivation_failed_error_code_is_key_derivation_failed() {
        let err = CryptoError::DerivationFailed;
        assert_eq!(err.to_error_code(), ErrorCode::CryptoKeyDerivationFailed);
        assert_eq!(
            err.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn only_encryption_failed_is_fatal() {
        assert_eq!(
            CryptoError::EncryptionFailed.to_error_code().level(),
            crate::errors::ErrorLevel::Fatal
        );
        assert_eq!(
            CryptoError::DecryptionFailed.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
        assert_eq!(
            CryptoError::InvalidKey.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
        assert_eq!(
            CryptoError::InvalidNonce.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
        assert_eq!(
            CryptoError::DerivationFailed.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn crypto_error_converts_to_service_error_box() {
        let err = CryptoError::DecryptionFailed;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code(), ErrorCode::CryptoDecryptionFailed);
        assert_eq!(
            boxed.to_error_code().level(),
            crate::errors::ErrorLevel::Operation
        );
    }

    #[test]
    fn encryption_failed_converts_to_fatal_service_error() {
        let err = CryptoError::EncryptionFailed;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code(), ErrorCode::CryptoEncryptionFailed);
        assert_eq!(
            boxed.to_error_code().level(),
            crate::errors::ErrorLevel::Fatal
        );
    }

    #[test]
    fn error_context_is_empty() {
        let err = CryptoError::InvalidKey;
        let ctx = err.to_error_context();
        assert!(ctx.record_id.is_none());
        assert!(ctx.record_name.is_none());
    }

    #[test]
    fn fallback_messages_are_set() {
        assert_eq!(
            CryptoError::EncryptionFailed.to_fallback_message(),
            "Encryption failed"
        );
        assert_eq!(
            CryptoError::DecryptionFailed.to_fallback_message(),
            "Decryption failed"
        );
        assert_eq!(CryptoError::InvalidKey.to_fallback_message(), "Invalid key");
        assert_eq!(
            CryptoError::InvalidNonce.to_fallback_message(),
            "Invalid nonce"
        );
        assert_eq!(
            CryptoError::DerivationFailed.to_fallback_message(),
            "Key derivation failed"
        );
    }
}
