use argon2::{Argon2, Params, Version};
use subtle::ConstantTimeEq;

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
    derive_key_with_params(password.get(), salt, &Argon2Params::medium())
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
        assert_eq!(salt.len(), 16, "salt must always be 16 bytes");
    }

    #[test]
    fn test_generate_salt_randomness() {
        let a = generate_salt();
        let b = generate_salt();
        assert_ne!(a, b, "two consecutive salts must differ");
    }

    #[test]
    fn test_derive_key_determinism() {
        let salt = generate_salt();
        let k1 = derive_key("hunter2", &salt).unwrap();
        let k2 = derive_key("hunter2", &salt).unwrap();
        assert_eq!(k1, k2, "same password + salt must produce same key");
    }

    #[test]
    fn test_derive_key_different_password() {
        let salt = generate_salt();
        let k1 = derive_key("password_a", &salt).unwrap();
        let k2 = derive_key("password_b", &salt).unwrap();
        assert_ne!(k1, k2, "different passwords must produce different keys");
    }

    #[test]
    fn test_derive_key_different_salt() {
        let s1 = generate_salt();
        let s2 = generate_salt();
        let k1 = derive_key("same_password", &s1).unwrap();
        let k2 = derive_key("same_password", &s2).unwrap();
        assert_ne!(k1, k2, "different salts must produce different keys");
    }

    #[test]
    fn test_derive_key_length() {
        let salt = generate_salt();
        let key = derive_key("test_password", &salt).unwrap();
        assert_eq!(key.len(), 32, "derived key must be 32 bytes");
    }

    #[test]
    fn test_derive_key_sensitive_matches_derive_key() {
        let salt = generate_salt();
        let plain = derive_key("my_secret", &salt).unwrap();
        let secure = derive_key_sensitive(&SecureStr::new("my_secret".to_string()), &salt).unwrap();
        assert_eq!(plain, secure, "SecureStr version must match plain version");
    }

    #[test]
    fn test_hash_password_roundtrip() {
        let hash = hash_password("correct_horse_battery").unwrap();
        let ok = verify_password("correct_horse_battery", &hash).unwrap();
        assert!(ok, "verify must succeed for the same password");
    }

    #[test]
    fn test_verify_wrong_password() {
        let hash = hash_password("correct_password").unwrap();
        let ok = verify_password("wrong_password", &hash).unwrap();
        assert!(!ok, "verify must fail for a different password");
    }

    #[test]
    fn test_params_high() {
        let p = Argon2Params::high();
        assert_eq!((p.m_cost, p.t_cost, p.p_cost), (65536, 3, 2));
    }

    #[test]
    fn test_params_medium() {
        let p = Argon2Params::medium();
        assert_eq!((p.m_cost, p.t_cost, p.p_cost), (49152, 2, 2));
    }

    #[test]
    fn test_params_low() {
        let p = Argon2Params::low();
        assert_eq!((p.m_cost, p.t_cost, p.p_cost), (32768, 2, 1));
    }

    #[test]
    fn test_constant_time_verify() {
        // Security: verify_password uses subtle::ConstantTimeEq to prevent timing attacks.
        // This test confirms the wiring is correct by exercising ct_eq directly.
        let hash = hash_password("test_pw").unwrap();
        let derived = derive_key_with_params("test_pw", &hash.salt, &hash.params).unwrap();
        assert!(bool::from(derived.ct_eq(&hash.key)));
        let wrong = derive_key_with_params("other", &hash.salt, &hash.params).unwrap();
        assert!(!bool::from(wrong.ct_eq(&hash.key)));
    }
}
