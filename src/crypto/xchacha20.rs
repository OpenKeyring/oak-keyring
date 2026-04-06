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

    fn generate_test_key() -> [u8; 32] {
        [1u8; 32]
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = generate_test_key();
        let plaintext = b"Hello, World!";

        let (ciphertext, nonce) = encrypt(plaintext, &key).expect("encryption failed");
        let decrypted = decrypt(&ciphertext, &nonce, &key).expect("decryption failed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_with_aad_roundtrip() {
        let key = generate_test_key();
        let plaintext = b"Sensitive data";
        let aad = b"additional authenticated data";

        let encrypted = encrypt_with_aad(plaintext, aad, &key).expect("encryption failed");
        let decrypted = decrypt_with_aad(&encrypted, aad, &key).expect("decryption failed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_nonce_per_encrypt() {
        let key = generate_test_key();
        let plaintext = b"Same plaintext";

        let (_, nonce1) = encrypt(plaintext, &key).expect("first encryption failed");
        let (_, nonce2) = encrypt(plaintext, &key).expect("second encryption failed");

        assert_ne!(nonce1, nonce2);
    }

    #[test]
    fn test_different_ciphertext_per_encrypt() {
        let key = generate_test_key();
        let plaintext = b"Same plaintext";

        let (ciphertext1, _) = encrypt(plaintext, &key).expect("first encryption failed");
        let (ciphertext2, _) = encrypt(plaintext, &key).expect("second encryption failed");

        assert_ne!(ciphertext1, ciphertext2);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = generate_test_key();
        let key2 = [2u8; 32];
        let plaintext = b"Secret message";

        let (ciphertext, nonce) = encrypt(plaintext, &key1).expect("encryption failed");
        let result = decrypt(&ciphertext, &nonce, &key2);

        assert!(result.is_err(), "Decryption with wrong key should fail");
    }

    #[test]
    fn test_decrypt_wrong_nonce_fails() {
        let key = generate_test_key();
        let plaintext = b"Secret message";

        let (ciphertext, _) = encrypt(plaintext, &key).expect("encryption failed");
        let wrong_nonce = [0u8; 24];
        let result = decrypt(&ciphertext, &wrong_nonce, &key);

        assert!(result.is_err(), "Decryption with wrong nonce should fail");
    }

    #[test]
    fn test_decrypt_with_wrong_aad_fails() {
        let key = generate_test_key();
        let plaintext = b"Sensitive data";
        let aad1 = b"correct aad";
        let aad2 = b"wrong aad";

        let encrypted = encrypt_with_aad(plaintext, aad1, &key).expect("encryption failed");
        let result = decrypt_with_aad(&encrypted, aad2, &key);

        assert!(result.is_err(), "Decryption with wrong AAD should fail");
    }

    #[test]
    fn test_decrypt_modified_ciphertext_fails() {
        let key = generate_test_key();
        let plaintext = b"Secret message";

        let (mut ciphertext, nonce) = encrypt(plaintext, &key).expect("encryption failed");

        if !ciphertext.is_empty() {
            ciphertext[0] ^= 0xFF;
        }

        let result = decrypt(&ciphertext, &nonce, &key);
        assert!(
            result.is_err(),
            "Decryption of modified ciphertext should fail"
        );
    }

    #[test]
    fn test_empty_plaintext() {
        let key = generate_test_key();
        let plaintext = b"";

        let (ciphertext, nonce) = encrypt(plaintext, &key).expect("encryption failed");
        let decrypted = decrypt(&ciphertext, &nonce, &key).expect("decryption failed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_large_plaintext() {
        let key = generate_test_key();
        let plaintext = vec![0xAB; 64 * 1024];

        let (ciphertext, nonce) = encrypt(&plaintext, &key).expect("encryption failed");
        let decrypted = decrypt(&ciphertext, &nonce, &key).expect("decryption failed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_nonce_length_24_bytes() {
        let key = generate_test_key();
        let plaintext = b"Test";

        let (_, nonce) = encrypt(plaintext, &key).expect("encryption failed");

        assert_eq!(nonce.len(), 24);
    }

    #[test]
    fn test_ciphertext_longer_than_plaintext() {
        let key = generate_test_key();
        let plaintext = b"Test message";

        let (ciphertext, _) = encrypt(plaintext, &key).expect("encryption failed");

        // Poly1305 adds a 16-byte authentication tag
        assert!(ciphertext.len() > plaintext.len());
        assert!(ciphertext.len() >= plaintext.len() + 16);
    }
}
