//! OpVault cryptographic operations: PBKDF2 key derivation, opdata01 decryption,
//! and item key decryption.

use aes::Aes256;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use cbc::cipher::{generic_array::GenericArray, BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use sha2::{Digest, Sha256, Sha512};

use super::types::{DecryptedKeys, KeyPair, Profile};
use crate::errors::mapping::import_export::ImportExportError;

type HmacSha256 = Hmac<Sha256>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;

/// Derive encryption and HMAC keys from master password using PBKDF2-HMAC-SHA512.
pub fn derive_keys(password: &[u8], salt: &[u8], iterations: u32) -> KeyPair {
    let mut out = [0u8; 64];
    pbkdf2_hmac::<Sha512>(password, salt, iterations, &mut out);

    let mut enc = [0u8; 32];
    let mut mac = [0u8; 32];
    enc.copy_from_slice(&out[..32]);
    mac.copy_from_slice(&out[32..]);

    KeyPair { enc, mac }
}

/// Derive a composite key pair from decrypted key material via SHA-512.
pub fn composite_key_from_material(material: &[u8]) -> KeyPair {
    let digest = Sha512::digest(material);

    let mut enc = [0u8; 32];
    let mut mac = [0u8; 32];
    enc.copy_from_slice(&digest[..32]);
    mac.copy_from_slice(&digest[32..64]);

    KeyPair { enc, mac }
}

/// Decrypt opdata01 binary data.
///
/// opdata01 format: [8B "opdata01"][8B plaintext_len LE][16B IV][ciphertext][32B HMAC-SHA256]
/// OpVault uses front random padding: after decryption, take the last `plaintext_len` bytes.
pub fn decrypt_opdata01(
    data: &[u8],
    key: &[u8; 32],
    mac_key: &[u8; 32],
) -> Result<Vec<u8>, ImportExportError> {
    const HEADER: &[u8; 8] = b"opdata01";
    const MIN_LEN: usize = 8 + 8 + 16 + 16 + 32; // header + len + iv + min cipher block + hmac

    if data.len() < MIN_LEN {
        return Err(ImportExportError::DecryptionFailed(
            "opdata01: data too short".into(),
        ));
    }

    // Verify HMAC over everything except the last 32 bytes (the HMAC itself).
    let signed_end = data.len() - 32;
    verify_hmac(&data[..signed_end], &data[signed_end..], mac_key)?;

    if &data[..8] != HEADER {
        return Err(ImportExportError::DecryptionFailed(
            "opdata01: invalid header".into(),
        ));
    }

    let plain_len = u64::from_le_bytes(
        data[8..16]
            .try_into()
            .map_err(|_| ImportExportError::DecryptionFailed("opdata01: length parse".into()))?,
    ) as usize;
    let iv = &data[16..32];
    let ciphertext = &data[32..signed_end];

    if !ciphertext.len().is_multiple_of(16) {
        return Err(ImportExportError::DecryptionFailed(
            "opdata01: ciphertext not block-aligned".into(),
        ));
    }
    if plain_len > ciphertext.len() {
        return Err(ImportExportError::DecryptionFailed(
            "opdata01: plaintext length exceeds ciphertext".into(),
        ));
    }

    let mut buf = ciphertext.to_vec();
    let mut decryptor = Aes256CbcDec::new_from_slices(key, iv)
        .map_err(|e| ImportExportError::DecryptionFailed(format!("opdata01: cipher init: {e}")))?;

    // Decrypt all blocks (no standard padding — opdata01 uses front random padding).
    for chunk in buf.chunks_exact_mut(16) {
        decryptor.decrypt_block_mut(GenericArray::from_mut_slice(chunk));
    }

    // Take last plain_len bytes (front random padding is discarded).
    let padding_len = buf.len() - plain_len;
    Ok(buf[padding_len..].to_vec())
}

/// Decrypt opdata01 from a base64-encoded string.
pub fn decrypt_opdata01_b64(
    b64: &str,
    key: &[u8; 32],
    mac_key: &[u8; 32],
) -> Result<Vec<u8>, ImportExportError> {
    let bytes = STANDARD
        .decode(b64)
        .map_err(|e| ImportExportError::DecryptionFailed(format!("base64 decode: {e}")))?;
    decrypt_opdata01(&bytes, key, mac_key)
}

/// Decrypt an item key from the `k` field.
///
/// Item key format (NOT opdata01): [16B IV][64B AES-256-CBC encrypted][32B HMAC-SHA256]
/// Decrypted 64 bytes split into: item_enc_key(32) + item_mac_key(32).
pub fn decrypt_item_key(k_data: &[u8], master: &KeyPair) -> Result<KeyPair, ImportExportError> {
    // Expected: 16 (IV) + 64 (encrypted keys) + 32 (HMAC) = 112 bytes
    if k_data.len() != 112 {
        return Err(ImportExportError::DecryptionFailed(format!(
            "item key: expected 112 bytes, got {}",
            k_data.len()
        )));
    }

    let signed_part = &k_data[..80]; // IV + encrypted keys
    let expected_mac = &k_data[80..112];

    verify_hmac(signed_part, expected_mac, &master.mac)?;

    let iv = &k_data[..16];
    let encrypted_keys = &k_data[16..80];

    let mut buf = encrypted_keys.to_vec();
    let mut decryptor = Aes256CbcDec::new_from_slices(&master.enc, iv)
        .map_err(|e| ImportExportError::DecryptionFailed(format!("item key: cipher init: {e}")))?;

    for chunk in buf.chunks_exact_mut(16) {
        decryptor.decrypt_block_mut(GenericArray::from_mut_slice(chunk));
    }

    let mut enc = [0u8; 32];
    let mut mac = [0u8; 32];
    enc.copy_from_slice(&buf[..32]);
    mac.copy_from_slice(&buf[32..64]);

    Ok(KeyPair { enc, mac })
}

/// Decrypt item key from a base64-encoded string.
pub fn decrypt_item_key_b64(k_b64: &str, master: &KeyPair) -> Result<KeyPair, ImportExportError> {
    let k = STANDARD
        .decode(k_b64)
        .map_err(|e| ImportExportError::DecryptionFailed(format!("item key base64: {e}")))?;
    decrypt_item_key(&k, master)
}

/// Decrypt all keys from profile.js using the master password.
pub fn decrypt_keys_from_profile(
    profile: &Profile,
    password: &[u8],
) -> Result<DecryptedKeys, ImportExportError> {
    let lock = profile
        .resolve_lock()
        .map_err(|e| ImportExportError::ParseError {
            format: "opvault profile".into(),
            reason: e,
        })?;

    let salt = STANDARD
        .decode(&lock.salt)
        .map_err(|e| ImportExportError::DecryptionFailed(format!("salt base64: {e}")))?;

    let derived = derive_keys(password, &salt, lock.iterations);

    // Decrypt master key → SHA-512 → master key pair.
    let master_material = decrypt_opdata01_b64(&lock.master_key, &derived.enc, &derived.mac)?;
    let master = composite_key_from_material(&master_material);

    // Decrypt overview key → SHA-512 → overview key pair.
    let overview_material = decrypt_opdata01_b64(&lock.overview_key, &derived.enc, &derived.mac)?;
    let overview = composite_key_from_material(&overview_material);

    Ok(DecryptedKeys { master, overview })
}

/// Verify HMAC-SHA256 over data against expected mac.
fn verify_hmac(
    data: &[u8],
    expected_mac: &[u8],
    mac_key: &[u8; 32],
) -> Result<(), ImportExportError> {
    let mut mac = HmacSha256::new_from_slice(mac_key)
        .map_err(|e| ImportExportError::DecryptionFailed(format!("HMAC init: {e}")))?;
    mac.update(data);
    mac.verify_slice(expected_mac)
        .map_err(|_| ImportExportError::InvalidPassword)
}
