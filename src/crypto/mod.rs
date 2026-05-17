pub mod argon2;
pub mod bip39;
pub mod crypto_manager;
#[cfg(feature = "sqlcipher")]
pub mod db_page_key;
pub mod hkdf;
pub mod keystore;
pub mod password;
pub mod payload;
pub mod self_test;
pub mod strength;
pub mod xchacha20;

pub use bip39::MnemonicLanguage;

// =============================================================================
// CryptoError — unified error type for the entire crypto layer
// =============================================================================

/// Unified, sanitized error type for the crypto layer.
///
/// Never carries implementation details (e.g., "chacha20", "poly1305", "argon2").
/// All variants are opaque — callers see only the category of failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// AEAD or KDF decryption failed (wrong key, tampered ciphertext, wrong password).
    DecryptionFailed,
    /// AEAD or KDF encryption/derivation failed.
    EncryptionFailed,
    /// The provided key is invalid (wrong length or encoding).
    InvalidKey,
    /// The provided nonce is invalid (wrong length or encoding).
    InvalidNonce,
    /// A key derivation operation (HKDF/Argon2) failed.
    DerivationFailed,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::DecryptionFailed => write!(f, "decryption failed"),
            CryptoError::EncryptionFailed => write!(f, "encryption failed"),
            CryptoError::InvalidKey => write!(f, "invalid key"),
            CryptoError::InvalidNonce => write!(f, "invalid nonce"),
            CryptoError::DerivationFailed => write!(f, "key derivation failed"),
        }
    }
}

impl std::error::Error for CryptoError {}

impl From<CryptoError> for String {
    fn from(e: CryptoError) -> String {
        e.to_string()
    }
}

impl From<String> for CryptoError {
    fn from(_: String) -> CryptoError {
        CryptoError::DerivationFailed
    }
}

impl From<&str> for CryptoError {
    fn from(_: &str) -> CryptoError {
        CryptoError::DerivationFailed
    }
}

pub use crypto_manager::CryptoManager;
pub use keystore::{
    unwrap_key, wrap_key, DataEncryptionKey, DeviceKey, KeyEncryptionKey, KeyStore, SecretKey,
    WrappingKey,
};
pub use xchacha20::{decrypt, decrypt_with_aad, encrypt, encrypt_with_aad, EncryptedData};
