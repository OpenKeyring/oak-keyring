use crate::crypto::CryptoError;
use crate::errors::service_error::ServiceError;
use crate::errors::ErrorCode;

impl ServiceError for CryptoError {
    fn to_error_code(&self) -> ErrorCode {
        match self {
            CryptoError::DecryptionFailed => ErrorCode::CryptoDecryptionFailed,
            CryptoError::EncryptionFailed => ErrorCode::CryptoEncryptionFailed,
            CryptoError::InvalidKey => ErrorCode::CryptoKeyDerivationFailed,
            CryptoError::InvalidNonce => ErrorCode::CryptoInvalidNonce,
            CryptoError::DerivationFailed => ErrorCode::CryptoKeyDerivationFailed,
        }
    }

    // CryptoError variants are opaque unit types — they carry no structured data.
    // Context (record_id, record_name) should be attached by the service layer
    // when wrapping CryptoError into a higher-level error type.
    fn to_error_context(&self) -> crate::errors::ErrorContext {
        crate::errors::ErrorContext::new()
    }

    fn to_fallback_message(&self) -> String {
        match self {
            CryptoError::DecryptionFailed => {
                "Decryption failed. The data may be corrupted or the wrong key was used".to_string()
            }
            CryptoError::EncryptionFailed => "Encryption failed due to a system error".to_string(),
            CryptoError::InvalidKey => {
                "Invalid cryptographic key. The key may be malformed or have an incorrect length"
                    .to_string()
            }
            CryptoError::InvalidNonce => {
                "Invalid nonce. The nonce may be malformed or have an incorrect length".to_string()
            }
            CryptoError::DerivationFailed => {
                "Key derivation failed. The password or parameters may be invalid".to_string()
            }
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
    use crate::errors::ErrorLevel;

    #[test]
    fn decryption_failed_error_code_is_crypto_decryption_failed() {
        let err = CryptoError::DecryptionFailed;
        assert_eq!(err.to_error_code(), ErrorCode::CryptoDecryptionFailed);
    }

    #[test]
    fn encryption_failed_is_fatal() {
        let err = CryptoError::EncryptionFailed;
        assert_eq!(err.to_error_code(), ErrorCode::CryptoEncryptionFailed);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Fatal);
    }

    #[test]
    fn invalid_key_error_code_is_crypto_key_derivation_failed() {
        let err = CryptoError::InvalidKey;
        assert_eq!(err.to_error_code(), ErrorCode::CryptoKeyDerivationFailed);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn invalid_nonce_error_code_is_crypto_invalid_nonce() {
        let err = CryptoError::InvalidNonce;
        assert_eq!(err.to_error_code(), ErrorCode::CryptoInvalidNonce);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn derivation_failed_error_code_is_crypto_key_derivation_failed() {
        let err = CryptoError::DerivationFailed;
        assert_eq!(err.to_error_code(), ErrorCode::CryptoKeyDerivationFailed);
        assert_eq!(err.to_error_code().level(), ErrorLevel::Operation);
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
            assert_eq!(v.to_error_code().level(), ErrorLevel::Operation);
        }
        assert_eq!(
            CryptoError::EncryptionFailed.to_error_code().level(),
            ErrorLevel::Fatal
        );
    }

    #[test]
    fn crypto_error_converts_to_service_error_box() {
        let err = CryptoError::DecryptionFailed;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code(), ErrorCode::CryptoDecryptionFailed);
        assert_eq!(boxed.to_error_code().level(), ErrorLevel::Operation);
    }

    #[test]
    fn encryption_failed_converts_to_fatal_service_error() {
        let err = CryptoError::EncryptionFailed;
        let boxed: crate::errors::ServiceErrorBox = err.into();
        assert_eq!(boxed.to_error_code(), ErrorCode::CryptoEncryptionFailed);
        assert_eq!(boxed.to_error_code().level(), ErrorLevel::Fatal);
    }

    #[test]
    fn error_context_is_none() {
        let err = CryptoError::InvalidKey;
        assert!(err.to_error_context().to_interpolation_map().is_empty());
    }
}
