use argon2::{Argon2, Params, Version};
use subtle::ConstantTimeEq;

use crate::security::LockedKey32;
use crate::types::sensitive::SecureStr;

#[derive(Debug, Clone, Copy)]
pub struct Argon2Params {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Argon2Params {
    pub fn high() -> Self {
        Self {
            m_cost: 65536,
            t_cost: 3,
            p_cost: 2,
        }
    }
    pub fn medium() -> Self {
        Self {
            m_cost: 49152,
            t_cost: 2,
            p_cost: 2,
        }
    }
    pub fn low() -> Self {
        Self {
            m_cost: 32768,
            t_cost: 2,
            p_cost: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PasswordHash {
    pub salt: [u8; 16],
    pub key: Vec<u8>,
    pub params: Argon2Params,
}

pub fn generate_salt() -> [u8; 16] {
    use chacha20poly1305::aead::rand_core::RngCore;
    let mut salt = [0u8; 16];
    chacha20poly1305::aead::OsRng.fill_bytes(&mut salt);
    salt
}

pub fn derive_key(password: &str, salt: &[u8; 16]) -> Result<Vec<u8>, String> {
    derive_key_with_params(password, salt, &Argon2Params::medium())
}

pub fn derive_key_sensitive(password: &SecureStr, salt: &[u8; 16]) -> Result<Vec<u8>, String> {
    derive_key_with_params(password.expose(), salt, &Argon2Params::medium())
}

pub fn derive_key_with_params(
    password: &str,
    salt: &[u8; 16],
    params: &Argon2Params,
) -> Result<Vec<u8>, String> {
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        Version::V0x13,
        Params::new(params.m_cost, params.t_cost, params.p_cost, Some(32))
            .map_err(|e| e.to_string())?,
    );

    let mut key = vec![0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| e.to_string())?;

    Ok(key)
}

/// Derives a 32-byte key using Argon2id, outputting directly into locked memory.
///
/// This function is the secure version of [`derive_key_with_params`] for
/// sensitive key derivation (e.g., master keys, wrapping keys). It ensures
/// the derived key material is never written to swap or core dumps.
///
/// # Arguments
///
/// * `password` - The password as a SecureStr (zeroized on drop)
/// * `salt` - 16-byte salt for key derivation
/// * `params` - Argon2 parameters (memory cost, time cost, parallelism)
///
/// # Returns
///
/// A [`LockedKey32`] containing the derived key in locked memory.
///
/// # Errors
///
/// Returns an error if:
/// - Argon2 parameter construction fails
/// - Memory locking fails (OS resource limits)
/// - Key derivation fails
///
/// # Examples
///
/// ```no_run
/// use oak_keyring::crypto::argon2::{self, Argon2Params};
/// use oak_keyring::types::SecureStr;
///
/// let password = SecureStr::new("master-password".to_string());
/// let salt = [1u8; 16];
/// let key = argon2::derive_key_locked(&password, &salt, &Argon2Params::medium())
///     .expect("key derivation should succeed");
/// // The key is now in locked memory and will be zeroized on drop
/// ```
pub fn derive_key_locked(
    password: &SecureStr,
    salt: &[u8; 16],
    params: &Argon2Params,
) -> Result<LockedKey32, String> {
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        Version::V0x13,
        Params::new(params.m_cost, params.t_cost, params.p_cost, Some(32))
            .map_err(|e| e.to_string())?,
    );

    LockedKey32::generate_from(|out| {
        argon2
            .hash_password_into(password.expose().as_bytes(), salt, out)
            .map_err(|e| e.to_string())
    })
    .map_err(|e| e.to_string())
}

pub fn hash_password(password: &str) -> Result<PasswordHash, String> {
    let params = Argon2Params::medium();
    let salt = generate_salt();
    let key = derive_key_with_params(password, &salt, &params)?;
    Ok(PasswordHash { salt, key, params })
}

pub fn verify_password(password: &str, hash: &PasswordHash) -> Result<bool, String> {
    let derived = derive_key_with_params(password, &hash.salt, &hash.params)?;
    Ok(derived.ct_eq(&hash.key).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_salt_length() {
        let salt = generate_salt();
        assert_eq!(salt.len(), 16, "Salt length should always be 16 bytes");
    }

    #[test]
    fn test_generate_salt_randomness() {
        let salt1 = generate_salt();
        let salt2 = generate_salt();
        assert_ne!(
            salt1, salt2,
            "Two consecutive salt generations should produce different values"
        );
    }

    #[test]
    fn test_derive_key_determinism() {
        let password = "test_password_123";
        let salt = generate_salt();

        let key1 = derive_key(password, &salt).expect("First derivation should succeed");
        let key2 = derive_key(password, &salt).expect("Second derivation should succeed");

        assert_eq!(key1, key2, "Same password + salt should produce same key");
    }

    #[test]
    fn test_derive_key_different_password() {
        let salt = generate_salt();

        let key1 = derive_key("password_one", &salt).expect("First derivation should succeed");
        let key2 = derive_key("password_two", &salt).expect("Second derivation should succeed");

        assert_ne!(
            key1, key2,
            "Different passwords should produce different keys"
        );
    }

    #[test]
    fn test_derive_key_different_salt() {
        let password = "same_password";
        let salt1 = generate_salt();
        let salt2 = generate_salt();

        let key1 = derive_key(password, &salt1).expect("First derivation should succeed");
        let key2 = derive_key(password, &salt2).expect("Second derivation should succeed");

        assert_ne!(key1, key2, "Different salts should produce different keys");
    }

    #[test]
    fn test_derive_key_length() {
        let password = "test_password";
        let salt = generate_salt();

        let key = derive_key(password, &salt).expect("Derivation should succeed");

        assert_eq!(key.len(), 32, "Output key length should always be 32 bytes");
    }

    #[test]
    fn test_derive_key_sensitive_matches_derive_key() {
        let password = "test_sensitive_password";
        let salt = generate_salt();

        let key_normal = derive_key(password, &salt).expect("Normal derivation should succeed");
        let secure_password = SecureStr::new(password.to_string());
        let key_sensitive = derive_key_sensitive(&secure_password, &salt)
            .expect("Sensitive derivation should succeed");

        assert_eq!(
            key_normal, key_sensitive,
            "SecureStr version should produce same result as normal version"
        );
    }

    #[test]
    fn test_hash_password_roundtrip() {
        let password = "correct_password";

        let hash = hash_password(password).expect("Hashing should succeed");
        let is_valid = verify_password(password, &hash).expect("Verification should succeed");

        assert!(
            is_valid,
            "hash_password + verify_password should pass for correct password"
        );
    }

    #[test]
    fn test_verify_wrong_password() {
        let password = "correct_password";
        let wrong_password = "wrong_password";

        let hash = hash_password(password).expect("Hashing should succeed");
        let is_valid = verify_password(wrong_password, &hash).expect("Verification should succeed");

        assert!(!is_valid, "Wrong password verification should fail");
    }

    #[test]
    fn test_params_high() {
        let params = Argon2Params::high();
        assert_eq!(params.m_cost, 65536, "High m_cost should be 65536");
        assert_eq!(params.t_cost, 3, "High t_cost should be 3");
        assert_eq!(params.p_cost, 2, "High p_cost should be 2");

        let salt = generate_salt();
        let result = derive_key_with_params("test", &salt, &params);
        assert!(result.is_ok(), "High params should work for key derivation");
    }

    #[test]
    fn test_params_medium() {
        let params = Argon2Params::medium();
        assert_eq!(params.m_cost, 49152, "Medium m_cost should be 49152");
        assert_eq!(params.t_cost, 2, "Medium t_cost should be 2");
        assert_eq!(params.p_cost, 2, "Medium p_cost should be 2");

        let salt = generate_salt();
        let result = derive_key_with_params("test", &salt, &params);
        assert!(
            result.is_ok(),
            "Medium params should work for key derivation"
        );
    }

    #[test]
    fn test_params_low() {
        let params = Argon2Params::low();
        assert_eq!(params.m_cost, 32768, "Low m_cost should be 32768");
        assert_eq!(params.t_cost, 2, "Low t_cost should be 2");
        assert_eq!(params.p_cost, 1, "Low p_cost should be 1");

        let salt = generate_salt();
        let result = derive_key_with_params("test", &salt, &params);
        assert!(result.is_ok(), "Low params should work for key derivation");
    }

    #[test]
    fn test_constant_time_verify() {
        // Security test: verify_password must use constant-time comparison to prevent timing attacks.
        // The implementation uses subtle::ConstantTimeEq::ct_eq() for this purpose.

        use subtle::ConstantTimeEq;

        let password = "test_password";
        let hash = hash_password(password).expect("Hashing should succeed");

        let derived = derive_key_with_params(password, &hash.salt, &hash.params)
            .expect("Derivation should succeed");

        let ct_result = derived.ct_eq(&hash.key);
        assert!(
            bool::from(ct_result),
            "Constant-time comparison should match for correct password"
        );

        let wrong_derived = derive_key_with_params("wrong", &hash.salt, &hash.params)
            .expect("Derivation should succeed");
        let ct_wrong = wrong_derived.ct_eq(&hash.key);
        assert!(
            !bool::from(ct_wrong),
            "Constant-time comparison should not match for wrong password"
        );
    }

    #[test]
    fn derive_key_locked_returns_32_byte_key() {
        let password = SecureStr::new("test_password".to_string());
        let salt = [1u8; 16];
        let key = derive_key_locked(&password, &salt, &Argon2Params::medium())
            .expect("locked derivation should succeed");
        assert_eq!(key.expose().len(), 32);
    }
}
