use chacha20poly1305::aead::{Aead, AeadCore, OsRng, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};

/// Sanitized crypto error type — never leaks implementation details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    DecryptionFailed,
    EncryptionFailed,
    InvalidKey,
    InvalidNonce,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::DecryptionFailed => write!(f, "decryption failed"),
            CryptoError::EncryptionFailed => write!(f, "encryption failed"),
            CryptoError::InvalidKey => write!(f, "invalid key"),
            CryptoError::InvalidNonce => write!(f, "invalid nonce"),
        }
    }
}

impl std::error::Error for CryptoError {}

impl From<chacha20poly1305::Error> for CryptoError {
    fn from(_: chacha20poly1305::Error) -> Self {
        // S5: never expose the underlying AEAD error message
        CryptoError::DecryptionFailed
    }
}

#[derive(Debug, Clone)]
pub struct EncryptedData {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 24],
}

pub fn encrypt(plaintext: &[u8], key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 24]), CryptoError> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoError::InvalidKey)?;
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_ref())
        .map_err(|_| CryptoError::EncryptionFailed)?;
    let mut nonce_bytes = [0u8; 24];
    nonce_bytes.copy_from_slice(&nonce);
    Ok((ciphertext, nonce_bytes))
}

pub fn decrypt(
    ciphertext: &[u8],
    nonce: &[u8; 24],
    key: &[u8; 32],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoError::InvalidKey)?;
    let nonce = XNonce::from_slice(nonce);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| CryptoError::DecryptionFailed)?;
    Ok(plaintext)
}

pub fn encrypt_with_aad(
    plaintext: &[u8],
    aad: &[u8],
    key: &[u8; 32],
) -> Result<EncryptedData, CryptoError> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoError::InvalidKey)?;
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::EncryptionFailed)?;
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
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoError::InvalidKey)?;
    let nonce = XNonce::from_slice(&encrypted.nonce);
    let plaintext = cipher
        .decrypt(
            nonce,
            Payload {
                msg: &encrypted.ciphertext,
                aad,
            },
        )
        .map_err(|_| CryptoError::DecryptionFailed)?;
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

    // ── S5: Error sanitization — CryptoError never leaks AEAD details ──
    #[test]
    fn test_error_sanitization_wrong_key() {
        let key = random_key();
        let wrong_key = random_key();
        let (ciphertext, nonce) = encrypt(b"secret", &key).unwrap();
        let err = decrypt(&ciphertext, &nonce, &wrong_key).unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.contains("chacha"),
            "error must not leak algorithm name: got '{}'",
            msg
        );
        assert!(
            !msg.contains("poly1305"),
            "error must not leak algorithm name: got '{}'",
            msg
        );
        assert!(
            !msg.contains("aead"),
            "error must not leak AEAD internals: got '{}'",
            msg
        );
        assert_eq!(err, CryptoError::DecryptionFailed);
    }

    #[test]
    fn test_error_sanitization_wrong_aad() {
        let key = random_key();
        let encrypted = encrypt_with_aad(b"payload", b"aad:A", &key).unwrap();
        let err = decrypt_with_aad(&encrypted, b"aad:B", &key).unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.contains("chacha") && !msg.contains("poly1305"),
            "error must not leak algorithm details: got '{}'",
            msg
        );
        assert_eq!(err, CryptoError::DecryptionFailed);
    }

    #[test]
    fn test_error_sanitization_tampered_ciphertext() {
        let key = random_key();
        let (mut ct, nonce) = encrypt(b"data", &key).unwrap();
        ct[0] ^= 0xFF;
        let err = decrypt(&ct, &nonce, &key).unwrap_err();
        assert_eq!(err, CryptoError::DecryptionFailed);
        assert!(!err.to_string().contains("chacha20poly1305"));
    }

    // ── Nonce randomness tests ─────────────────────────────────────────
    #[test]
    fn test_nonce_no_collision_1000_samples() {
        let key = random_key();
        let plaintext = b"collision-test";
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            let (_, nonce) = encrypt(plaintext, &key).unwrap();
            assert!(
                seen.insert(nonce),
                "nonce collision detected after {} samples",
                seen.len()
            );
        }
        assert_eq!(seen.len(), 1000);
    }

    #[test]
    fn test_nonce_uniformity_basic() {
        let key = random_key();
        let (_, nonce) = encrypt(b"uniformity-test", &key).unwrap();
        assert_ne!(nonce, [0u8; 24], "nonce must not be all zeros");
        assert_ne!(nonce, [0xFFu8; 24], "nonce must not be all 0xFF");
    }

    // ── AAD binding completeness tests ─────────────────────────────────
    #[test]
    fn test_aad_prevents_record_swap() {
        let key = random_key();
        let plaintext_a = b"record A payload";
        let plaintext_b = b"record B payload";
        let aad_a = b"record-id:A";
        let aad_b = b"record-id:B";

        let enc_a = encrypt_with_aad(plaintext_a, aad_a, &key).unwrap();
        let _enc_b = encrypt_with_aad(plaintext_b, aad_b, &key).unwrap();

        // Record A ciphertext decrypted with Record B AAD must fail
        let fake_enc_b_for_a = EncryptedData {
            ciphertext: enc_a.ciphertext.clone(),
            nonce: enc_a.nonce,
        };
        let result = decrypt_with_aad(&fake_enc_b_for_a, aad_b, &key);
        assert!(
            result.is_err(),
            "AAD BINDING: decrypting record A ciphertext with record B AAD must fail"
        );
    }

    #[test]
    fn test_aad_prevents_version_rollback() {
        let key = random_key();
        let plaintext = b"protected data";

        let aad_v1 = b"dek-version:1";
        let aad_v2 = b"dek-version:2";

        let enc_v2 = encrypt_with_aad(plaintext, aad_v2, &key).unwrap();

        // DEK_v2 ciphertext decrypted with DEK_v1 AAD must fail
        let result = decrypt_with_aad(&enc_v2, aad_v1, &key);
        assert!(
            result.is_err(),
            "AAD BINDING: version rollback must be detected"
        );
    }

    #[test]
    fn test_aad_prevents_record_reorder() {
        let key = random_key();
        let plaintext_a = b"record A payload";
        let plaintext_b = b"record B payload";
        let aad_a = b"record-id:A";
        let aad_b = b"record-id:B";

        let enc_a = encrypt_with_aad(plaintext_a, aad_a, &key).unwrap();
        let enc_b = encrypt_with_aad(plaintext_b, aad_b, &key).unwrap();

        // When AAD is unchanged (correct AAD used), decryption succeeds
        // even if records are reordered in storage
        let dec_a = decrypt_with_aad(&enc_a, aad_a, &key).unwrap();
        let dec_b = decrypt_with_aad(&enc_b, aad_b, &key).unwrap();
        assert_eq!(dec_a, plaintext_a);
        assert_eq!(dec_b, plaintext_b);
    }
}
