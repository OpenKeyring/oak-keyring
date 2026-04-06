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
