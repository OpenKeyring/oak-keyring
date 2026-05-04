use std::path::Path;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::crypto::argon2;
use crate::crypto::xchacha20;
use crate::errors::mapping::import_export::ImportExportError;
use crate::types::SecureStr;

const OKB_VERSION: u32 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;

/// A single record for export serialization.
#[derive(Serialize, Deserialize)]
pub struct ExportRecord {
    pub id: String,
    pub credential_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_favorite: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// SSH public key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// SSH private key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    /// SSH passphrase
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
    /// API application ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// API secret key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
}

/// The full export payload.
#[derive(Serialize, Deserialize)]
pub struct ExportPayload {
    pub version: String,
    pub vault_id: String,
    pub exported_at: String,
    pub records: Vec<ExportRecord>,
}

/// Validate that the export password meets minimum length requirements.
///
/// Returns `Ok(())` if password is at least 8 characters.
pub fn validate_export_password(password: &SecureStr) -> Result<(), ImportExportError> {
    if password.get().len() < 8 {
        return Err(ImportExportError::InvalidPassword);
    }
    Ok(())
}

/// Validate that the output path has the correct extension and parent exists.
pub fn validate_export_path(path: &Path) -> Result<(), ImportExportError> {
    // Check extension is .okb
    match path.extension() {
        Some(ext) if ext == "okb" => {}
        _ => {
            return Err(ImportExportError::InvalidFormat(
                "output file must have .okb extension".to_string(),
            ));
        }
    }

    // Check parent directory exists
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            if !parent.exists() {
                return Err(ImportExportError::FileWriteError {
                    path: path.to_path_buf(),
                    reason: "parent directory does not exist".to_string(),
                });
            }
        }
        Some(_) => {
            // Empty parent means current directory, which always exists.
        }
        None => {
            return Err(ImportExportError::FileWriteError {
                path: path.to_path_buf(),
                reason: "cannot determine parent directory".to_string(),
            });
        }
    }

    Ok(())
}

/// Encrypt the export payload and write it to the output file atomically.
///
/// The .okb binary format:
/// ```text
/// [4 bytes: version (u32 LE)]
/// [16 bytes: salt]
/// [24 bytes: nonce]
/// [N bytes: encrypted ciphertext (includes 16 bytes Poly1305 tag)]
/// ```
///
/// Returns the total number of bytes written.
pub fn encrypt_and_write_okb(
    payload: &ExportPayload,
    password: &SecureStr,
    output_path: &Path,
) -> Result<usize, ImportExportError> {
    // 1. Serialize payload to JSON.
    let json_bytes = serde_json::to_vec(payload).map_err(|e| {
        ImportExportError::InternalError(format!("failed to serialize export payload: {e}"))
    })?;

    // 2. Generate salt.
    let salt = argon2::generate_salt();

    // 3. Derive DEK via Argon2id.
    let dek = argon2::derive_key(password.get(), &salt)
        .map_err(ImportExportError::KeyDerivationFailed)?;

    // Convert DEK to fixed-size array for xchacha20.
    let dek_arr: [u8; 32] = dek
        .as_slice()
        .try_into()
        .map_err(|_| ImportExportError::InternalError("DEK is not 32 bytes".to_string()))?;

    // 4. Encrypt JSON with XChaCha20-Poly1305.
    let (ciphertext, nonce) =
        xchacha20::encrypt(&json_bytes, &dek_arr).map_err(ImportExportError::EncryptionFailed)?;

    // Zeroize plaintext JSON bytes now that encryption is complete.
    let mut json_bytes = json_bytes;
    json_bytes.zeroize();
    let mut dek_arr = dek_arr;
    dek_arr.zeroize();

    // 5. Build binary buffer: version (LE) + salt + nonce + ciphertext.
    let mut buf = Vec::with_capacity(4 + SALT_LEN + NONCE_LEN + ciphertext.len());
    buf.extend_from_slice(&OKB_VERSION.to_le_bytes());
    buf.extend_from_slice(&salt);
    buf.extend_from_slice(&nonce);
    buf.extend_from_slice(&ciphertext);

    // 6. Atomic write: write to temp file first, then rename.
    let tmp_path = output_path.with_extension("okb.tmp");
    std::fs::write(&tmp_path, &buf).map_err(|e| ImportExportError::FileWriteError {
        path: tmp_path.clone(),
        reason: e.to_string(),
    })?;

    std::fs::rename(&tmp_path, output_path).map_err(|e| ImportExportError::FileWriteError {
        path: output_path.to_path_buf(),
        reason: format!("atomic rename failed: {e}"),
    })?;

    Ok(buf.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // -- Test helpers --

    fn sample_payload() -> ExportPayload {
        ExportPayload {
            version: "1.0".to_string(),
            vault_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            exported_at: "2026-04-12T12:00:00Z".to_string(),
            records: vec![ExportRecord {
                id: "660e8400-e29b-41d4-a716-446655440001".to_string(),
                credential_type: "login".to_string(),
                name: "Gmail".to_string(),
                username: Some("user@gmail.com".to_string()),
                password: Some("s3cret123".to_string()),
                url: Some("https://gmail.com".to_string()),
                notes: None,
                tags: Some(vec!["email".to_string()]),
                is_favorite: Some(true),
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            }],
        }
    }

    fn valid_password() -> SecureStr {
        SecureStr::new("password123".to_string())
    }

    // -- Test 1: validate_export_password accepts 8 chars --

    #[test]
    fn validate_export_password_accepts_8_chars() {
        let pw = SecureStr::new("12345678".to_string());
        assert!(validate_export_password(&pw).is_ok());
    }

    // -- Test 2: validate_export_password rejects short --

    #[test]
    fn validate_export_password_rejects_short() {
        let pw = SecureStr::new("1234567".to_string());
        let result = validate_export_password(&pw);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ImportExportError::InvalidPassword),
            "expected InvalidPassword"
        );
    }

    // -- Test 3: validate_export_path accepts .okb --

    #[test]
    fn validate_export_path_accepts_okb() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("export.okb");
        assert!(validate_export_path(&path).is_ok());
    }

    // -- Test 4: validate_export_path rejects non-.okb --

    #[test]
    fn validate_export_path_rejects_non_okb() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("export.txt");
        let result = validate_export_path(&path);
        assert!(result.is_err());
        assert!(
            matches!(
                result.unwrap_err(),
                ImportExportError::InvalidFormat(msg) if msg.contains(".okb")
            ),
            "expected InvalidFormat mentioning .okb"
        );
    }

    // -- Test 5: validate_export_path rejects missing parent --

    #[test]
    fn validate_export_path_rejects_missing_parent() {
        let path = PathBuf::from("/nonexistent_dir_abc123/export.okb");
        let result = validate_export_path(&path);
        assert!(result.is_err());
        assert!(
            matches!(
                result.unwrap_err(),
                ImportExportError::FileWriteError { reason, .. } if reason.contains("parent directory does not exist")
            ),
            "expected FileWriteError about missing parent"
        );
    }

    // -- Test 6: encrypt_and_write produces valid file --

    #[test]
    fn encrypt_and_write_produces_valid_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("export.okb");
        let payload = sample_payload();

        let bytes_written = encrypt_and_write_okb(&payload, &valid_password(), &path)
            .expect("write should succeed");

        // File should exist.
        assert!(path.exists(), "output file should exist");

        // Read back and verify header.
        let data = std::fs::read(&path).expect("read output file");

        // Minimum: 4 (version) + 16 (salt) + 24 (nonce) + 16 (tag) = 60 bytes.
        assert!(
            data.len() > 44,
            "file should be at least 44 bytes, got {}",
            data.len()
        );

        // Verify version bytes (1u32 LE).
        let version = u32::from_le_bytes(data[0..4].try_into().expect("4 bytes"));
        assert_eq!(version, 1u32, "version should be 1");

        // Verify reported size matches actual file size.
        assert_eq!(bytes_written, data.len());
    }

    // -- Test 7: roundtrip encrypt/decrypt --

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("roundtrip.okb");
        let payload = sample_payload();

        encrypt_and_write_okb(&payload, &valid_password(), &path).expect("write should succeed");

        // Read back and manually decrypt.
        let data = std::fs::read(&path).expect("read file");

        let _version = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let salt: [u8; 16] = data[4..20].try_into().unwrap();
        let nonce: [u8; 24] = data[20..44].try_into().unwrap();
        let ciphertext = &data[44..];

        // Derive the same DEK.
        let dek = argon2::derive_key(valid_password().get(), &salt).expect("derive key");
        let dek_arr: [u8; 32] = dek.as_slice().try_into().unwrap();

        // Decrypt.
        let plaintext =
            xchacha20::decrypt(ciphertext, &nonce, &dek_arr).expect("decrypt should succeed");

        // Deserialize and verify.
        let decrypted_payload: ExportPayload =
            serde_json::from_slice(&plaintext).expect("deserialize should succeed");

        assert_eq!(decrypted_payload.version, "1.0");
        assert_eq!(decrypted_payload.vault_id, payload.vault_id);
        assert_eq!(decrypted_payload.records.len(), 1);
        assert_eq!(decrypted_payload.records[0].name, "Gmail");
        assert_eq!(
            decrypted_payload.records[0].username.as_ref().unwrap(),
            "user@gmail.com"
        );
        // Verify new type-specific fields are None after roundtrip
        assert!(decrypted_payload.records[0].public_key.is_none());
        assert!(decrypted_payload.records[0].private_key.is_none());
        assert!(decrypted_payload.records[0].passphrase.is_none());
        assert!(decrypted_payload.records[0].app_id.is_none());
        assert!(decrypted_payload.records[0].secret_key.is_none());
    }

    // -- Test 8: encrypt_and_write atomic creates file --

    #[test]
    fn encrypt_and_write_atomic_creates_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("atomic.okb");
        let payload = sample_payload();

        encrypt_and_write_okb(&payload, &valid_password(), &path).expect("write should succeed");

        // The final file should exist.
        assert!(path.exists(), "output file should exist after atomic write");

        // The .tmp file should NOT exist.
        let tmp_path = path.with_extension("okb.tmp");
        assert!(
            !tmp_path.exists(),
            "temp file should have been renamed away"
        );
    }
}
