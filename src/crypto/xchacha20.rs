use chacha20poly1305::aead::{Aead, AeadCore, OsRng, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};

#[derive(Debug, Clone)]
pub struct EncryptedData {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 24],
}

pub fn encrypt(plaintext: &[u8], key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 24]), String> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_ref())
        .map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; 24];
    nonce_bytes.copy_from_slice(&nonce);
    Ok((ciphertext, nonce_bytes))
}

pub fn decrypt(ciphertext: &[u8], nonce: &[u8; 24], key: &[u8; 32]) -> Result<Vec<u8>, String> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = XNonce::from_slice(nonce);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| e.to_string())?;
    Ok(plaintext)
}

pub fn encrypt_with_aad(
    plaintext: &[u8],
    aad: &[u8],
    key: &[u8; 32],
) -> Result<EncryptedData, String> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            chacha20poly1305::aead::Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; 24];
    nonce_bytes.copy_from_slice(&nonce);
    Ok(EncryptedData {
        ciphertext,
        nonce: nonce_bytes,
    })
}

pub fn decrypt_with_aad(
    encrypted: &EncryptedData,
    aad: &[u8],
    key: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = XNonce::from_slice(&encrypted.nonce);
    let plaintext = cipher
        .decrypt(
            nonce,
            Payload {
                msg: &encrypted.ciphertext,
                aad,
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(plaintext)
}
