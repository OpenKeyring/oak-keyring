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

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    fn random_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        rand::rng().fill_bytes(&mut key);
        key
    }

    // ── Test 1: encrypt → decrypt roundtrip ──────────────────────────
    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = random_key();
        let plaintext = b"hello, xchacha20!";
        let (ciphertext, nonce) = encrypt(plaintext, &key).unwrap();
        let decrypted = decrypt(&ciphertext, &nonce, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // ── Test 2: encrypt_with_aad → decrypt_with_aad roundtrip ────────
    #[test]
    fn test_encrypt_with_aad_roundtrip() {
        let key = random_key();
        let plaintext = b"secret data";
        let aad = b"record-id:42";
        let encrypted = encrypt_with_aad(plaintext, aad, &key).unwrap();
        let decrypted = decrypt_with_aad(&encrypted, aad, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // ── Test 3: different nonce per encrypt ───────────────────────────
    #[test]
    fn test_different_nonce_per_encrypt() {
        let key = random_key();
        let plaintext = b"same plaintext";
        let (_, nonce1) = encrypt(plaintext, &key).unwrap();
        let (_, nonce2) = encrypt(plaintext, &key).unwrap();
        let (_, nonce3) = encrypt(plaintext, &key).unwrap();
        assert_ne!(nonce1, nonce2, "nonce1 and nonce2 must differ");
        assert_ne!(nonce2, nonce3, "nonce2 and nonce3 must differ");
        assert_ne!(nonce1, nonce3, "nonce1 and nonce3 must differ");
    }

    // ── Test 4: different ciphertext per encrypt ─────────────────────
    #[test]
    fn test_different_ciphertext_per_encrypt() {
        let key = random_key();
        let plaintext = b"same plaintext";
        let (ct1, _) = encrypt(plaintext, &key).unwrap();
        let (ct2, _) = encrypt(plaintext, &key).unwrap();
        let (ct3, _) = encrypt(plaintext, &key).unwrap();
        assert_ne!(ct1, ct2, "ciphertext1 and ciphertext2 must differ");
        assert_ne!(ct2, ct3, "ciphertext2 and ciphertext3 must differ");
        assert_ne!(ct1, ct3, "ciphertext1 and ciphertext3 must differ");
    }

    // ── Test 5: decrypt with wrong key fails ─────────────────────────
    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key = random_key();
        let wrong_key = random_key();
        let plaintext = b"sensitive data";
        let (ciphertext, nonce) = encrypt(plaintext, &key).unwrap();
        let result = decrypt(&ciphertext, &nonce, &wrong_key);
        assert!(result.is_err(), "decrypt with wrong key must return Err");
    }

    // ── Test 6: decrypt with wrong nonce fails ───────────────────────
    #[test]
    fn test_decrypt_wrong_nonce_fails() {
        let key = random_key();
        let plaintext = b"sensitive data";
        let (ciphertext, _) = encrypt(plaintext, &key).unwrap();
        let wrong_nonce = [0xABu8; 24];
        let result = decrypt(&ciphertext, &wrong_nonce, &key);
        assert!(result.is_err(), "decrypt with wrong nonce must return Err");
    }

    // ── Test 7: AAD BINDING — wrong AAD must fail ────────────────────
    #[test]
    fn test_decrypt_with_wrong_aad_fails() {
        let key = random_key();
        let plaintext = b"record payload";
        let aad_encrypt = b"record-id:42";
        let aad_wrong = b"record-id:99";
        let encrypted = encrypt_with_aad(plaintext, aad_encrypt, &key).unwrap();
        let result = decrypt_with_aad(&encrypted, aad_wrong, &key);
        assert!(
            result.is_err(),
            "AAD BINDING: decrypt with mismatched AAD must fail"
        );
    }

    // ── Test 8: modified ciphertext causes decrypt failure ───────────
    #[test]
    fn test_decrypt_modified_ciphertext_fails() {
        let key = random_key();
        let plaintext = b"important message";
        let (mut ciphertext, nonce) = encrypt(plaintext, &key).unwrap();
        if !ciphertext.is_empty() {
            ciphertext[0] ^= 0xFF;
        }
        let result = decrypt(&ciphertext, &nonce, &key);
        assert!(
            result.is_err(),
            "decrypt with tampered ciphertext must return Err"
        );
    }

    // ── Test 9: empty plaintext roundtrip ────────────────────────────
    #[test]
    fn test_empty_plaintext() {
        let key = random_key();
        let plaintext: &[u8] = b"";
        let (ciphertext, nonce) = encrypt(plaintext, &key).unwrap();
        let decrypted = decrypt(&ciphertext, &nonce, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // ── Test 10: large plaintext (64 KiB) roundtrip ──────────────────
    #[test]
    fn test_large_plaintext() {
        let key = random_key();
        let mut plaintext = vec![0u8; 64 * 1024];
        rand::rng().fill_bytes(&mut plaintext);
        let (ciphertext, nonce) = encrypt(&plaintext, &key).unwrap();
        let decrypted = decrypt(&ciphertext, &nonce, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // ── Test 11: nonce is always 24 bytes ────────────────────────────
    #[test]
    fn test_nonce_length_24_bytes() {
        let key = random_key();
        let (_, nonce) = encrypt(b"data", &key).unwrap();
        assert_eq!(nonce.len(), 24, "nonce must be exactly 24 bytes");
    }

    // ── Test 12: ciphertext longer than plaintext (Poly1305 tag) ─────
    #[test]
    fn test_ciphertext_longer_than_plaintext() {
        let key = random_key();
        let plaintext = b"hello world";
        let (ciphertext, _) = encrypt(plaintext, &key).unwrap();
        assert!(
            ciphertext.len() > plaintext.len(),
            "ciphertext ({}) must be longer than plaintext ({}) due to Poly1305 tag",
            ciphertext.len(),
            plaintext.len()
        );
    }
}
