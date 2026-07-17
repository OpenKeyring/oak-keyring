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
///
/// **Security**: sensitive fields (password, private_key, passphrase, secret_key)
/// are zeroized when this struct is dropped.
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
    pub totp: Option<String>,
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

impl Drop for ExportRecord {
    fn drop(&mut self) {
        if let Some(ref mut v) = self.password {
            v.zeroize();
        }
        if let Some(ref mut v) = self.private_key {
            v.zeroize();
        }
        if let Some(ref mut v) = self.passphrase {
            v.zeroize();
        }
        if let Some(ref mut v) = self.secret_key {
            v.zeroize();
        }
        if let Some(ref mut v) = self.totp {
            v.zeroize();
        }
    }
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
    if password.expose().len() < 8 {
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
    let dek = argon2::derive_key_locked(password, &salt, &argon2::Argon2Params::medium())
        .map_err(ImportExportError::KeyDerivationFailed)?;

    // 4. Encrypt JSON with XChaCha20-Poly1305.
    let (ciphertext, nonce) = xchacha20::encrypt(&json_bytes, dek.expose())
        .map_err(ImportExportError::EncryptionFailed)?;

    // Zeroize plaintext JSON bytes now that encryption is complete.
    let mut json_bytes = json_bytes;
    json_bytes.zeroize();

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

/// Write the export payload as a UTF-8 BOM CSV file.
///
/// The CSV includes all 15 columns for all credential types:
/// `id, credential_type, name, username, password, url, notes, tags, is_favorite, expires_at, public_key, private_key, passphrase, app_id, secret_key`
///
/// Empty/None fields are written as empty strings. Tags are joined with `;`.
/// The file starts with a UTF-8 BOM (EF BB BF) for Excel compatibility.
///
/// Returns the total number of bytes written.
pub fn write_csv(payload: &ExportPayload, output_path: &Path) -> Result<usize, ImportExportError> {
    let mut buf = Vec::new();

    // UTF-8 BOM for Excel compatibility.
    buf.extend_from_slice(&[0xEF, 0xBB, 0xBF]);

    {
        let mut wtr = csv::Writer::from_writer(&mut buf);

        // Header row.
        wtr.write_record([
            "id",
            "credential_type",
            "name",
            "username",
            "password",
            "url",
            "notes",
            "totp",
            "tags",
            "is_favorite",
            "expires_at",
            "public_key",
            "private_key",
            "passphrase",
            "app_id",
            "secret_key",
        ])
        .map_err(|e| ImportExportError::FileWriteError {
            path: output_path.to_path_buf(),
            reason: format!("failed to write CSV header: {e}"),
        })?;

        // Data rows.
        for record in &payload.records {
            let tags = record
                .tags
                .as_ref()
                .map(|t| t.join(";"))
                .unwrap_or_default();
            let is_favorite = record
                .is_favorite
                .map(|v| if v { "true" } else { "false" })
                .unwrap_or("");

            wtr.write_record([
                &record.id,
                &record.credential_type,
                &record.name,
                record.username.as_deref().unwrap_or(""),
                record.password.as_deref().unwrap_or(""),
                record.url.as_deref().unwrap_or(""),
                record.notes.as_deref().unwrap_or(""),
                record.totp.as_deref().unwrap_or(""),
                &tags,
                is_favorite,
                record.expires_at.as_deref().unwrap_or(""),
                record.public_key.as_deref().unwrap_or(""),
                record.private_key.as_deref().unwrap_or(""),
                record.passphrase.as_deref().unwrap_or(""),
                record.app_id.as_deref().unwrap_or(""),
                record.secret_key.as_deref().unwrap_or(""),
            ])
            .map_err(|e| ImportExportError::FileWriteError {
                path: output_path.to_path_buf(),
                reason: format!("failed to write CSV record: {e}"),
            })?;
        }

        wtr.flush().map_err(|e| ImportExportError::FileWriteError {
            path: output_path.to_path_buf(),
            reason: format!("failed to flush CSV: {e}"),
        })?;
    }

    // Atomic write: write to temp file first, then rename.
    let tmp_path = output_path.with_extension("csv.tmp");
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

/// Validate that the output path has .csv extension and parent exists.
pub fn validate_export_csv_path(path: &Path) -> Result<(), ImportExportError> {
    match path.extension() {
        Some(ext) if ext == "csv" => {}
        _ => {
            return Err(ImportExportError::InvalidFormat(
                "output file must have .csv extension".to_string(),
            ));
        }
    }

    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            if !parent.exists() {
                return Err(ImportExportError::FileWriteError {
                    path: path.to_path_buf(),
                    reason: "parent directory does not exist".to_string(),
                });
            }
        }
        Some(_) => {}
        None => {
            return Err(ImportExportError::FileWriteError {
                path: path.to_path_buf(),
                reason: "cannot determine parent directory".to_string(),
            });
        }
    }

    Ok(())
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
                totp: None,
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

    // -- Test: totp survives JSON serialization (OKB / JSON export round-trip) --

    #[test]
    fn export_payload_serializes_login_totp() {
        // A TOTP secret on a Login must survive JSON serialization so the
        // OKB/JSON export carries it and a re-import can recover it. The field
        // uses #[serde(skip_serializing_if = "Option::is_none")], so a present
        // value must still be emitted.
        let totp_uri = "otpauth://totp/GitHub:alice?secret=JBSWY3DPEHPK3PXP&issuer=GitHub";
        let payload = ExportPayload {
            version: "1.0".to_string(),
            vault_id: "vault-1".to_string(),
            exported_at: "2026-07-17T00:00:00Z".to_string(),
            records: vec![ExportRecord {
                id: "rec-1".to_string(),
                credential_type: "login".to_string(),
                name: "GitHub".to_string(),
                username: Some("alice".to_string()),
                password: Some("s3cret!".to_string()),
                url: None,
                notes: None,
                totp: Some(totp_uri.to_string()),
                tags: None,
                is_favorite: None,
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            }],
        };

        let json = serde_json::to_string(&payload).expect("serialize export payload");

        assert!(
            json.contains("\"totp\""),
            "serialized payload should include the totp field: {json}"
        );
        assert!(
            json.contains(totp_uri),
            "serialized payload should contain the totp uri verbatim: {json}"
        );
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
        let dek = argon2::derive_key(valid_password().expose(), &salt).expect("derive key");
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

    // -- Test 9: write_csv produces valid CSV with BOM and headers --

    #[test]
    fn write_csv_produces_valid_csv_with_bom() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("export.csv");
        let payload = sample_payload();

        write_csv(&payload, &path).expect("write should succeed");

        let data = std::fs::read(&path).expect("read output file");

        // Verify UTF-8 BOM
        assert_eq!(
            &data[0..3],
            &[0xEF, 0xBB, 0xBF],
            "file should start with UTF-8 BOM"
        );

        let text = std::str::from_utf8(&data[3..]).expect("valid UTF-8");
        let lines: Vec<&str> = text.lines().collect();
        assert!(!lines.is_empty(), "CSV should have at least a header line");

        // Verify header row contains all expected columns
        let header = lines[0];
        assert!(
            header.contains("credential_type"),
            "header should have credential_type"
        );
        assert!(header.contains("name"), "header should have name");
        assert!(header.contains("username"), "header should have username");
        assert!(header.contains("password"), "header should have password");
        assert!(header.contains("url"), "header should have url");
        assert!(header.contains("notes"), "header should have notes");
        assert!(header.contains("tags"), "header should have tags");
        assert!(
            header.contains("public_key"),
            "header should have public_key"
        );
        assert!(header.contains("app_id"), "header should have app_id");

        // Verify at least one data row
        assert!(lines.len() > 1, "CSV should have at least one data row");
    }

    // -- Test 10: write_csv handles all credential types --

    #[test]
    fn write_csv_handles_all_credential_types() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("export.csv");

        let payload = ExportPayload {
            version: "1.0".to_string(),
            vault_id: "test-vault".to_string(),
            exported_at: "2026-05-05T00:00:00Z".to_string(),
            records: vec![
                ExportRecord {
                    id: "1".to_string(),
                    credential_type: "login".to_string(),
                    name: "Gmail".to_string(),
                    username: Some("user@gmail.com".to_string()),
                    password: Some("s3cret".to_string()),
                    url: Some("https://gmail.com".to_string()),
                    notes: Some("My email".to_string()),
                    totp: None,
                    tags: Some(vec!["email".to_string()]),
                    is_favorite: Some(true),
                    expires_at: None,
                    public_key: None,
                    private_key: None,
                    passphrase: None,
                    app_id: None,
                    secret_key: None,
                },
                ExportRecord {
                    id: "2".to_string(),
                    credential_type: "ssh".to_string(),
                    name: "GitHub SSH".to_string(),
                    username: None,
                    password: None,
                    url: None,
                    notes: None,
                    totp: None,
                    tags: None,
                    is_favorite: None,
                    expires_at: None,
                    public_key: Some("ssh-rsa AAAA...".to_string()),
                    private_key: Some("-----BEGIN...".to_string()),
                    passphrase: Some("ssh_pass".to_string()),
                    app_id: None,
                    secret_key: None,
                },
                ExportRecord {
                    id: "3".to_string(),
                    credential_type: "api".to_string(),
                    name: "AWS API".to_string(),
                    username: None,
                    password: None,
                    url: None,
                    notes: None,
                    totp: None,
                    tags: Some(vec!["cloud".to_string(), "aws".to_string()]),
                    is_favorite: Some(false),
                    expires_at: None,
                    public_key: None,
                    private_key: None,
                    passphrase: None,
                    app_id: Some("AKIAIOSFODNN7".to_string()),
                    secret_key: Some("wJalrXUtnFEMI/K7MDENG".to_string()),
                },
            ],
        };

        write_csv(&payload, &path).expect("write should succeed");

        let data = std::fs::read_to_string(&path).expect("read file");
        // Skip BOM (3 bytes for UTF-8 BOM character)
        let text = &data[3..];
        let lines: Vec<&str> = text.lines().collect();

        // Header + 3 data rows
        assert_eq!(lines.len(), 4, "should have header + 3 data rows");

        // Verify login row
        assert!(lines[1].contains("user@gmail.com"));
        assert!(lines[1].contains("s3cret"));

        // Verify SSH row
        assert!(lines[2].contains("ssh-rsa AAAA..."));
        assert!(lines[2].contains("ssh_pass"));

        // Verify API row
        assert!(lines[3].contains("AKIAIOSFODNN7"));
        assert!(lines[3].contains("wJalrXUtnFEMI/K7MDENG"));
    }

    // -- Test 11: write_csv escapes special characters --

    #[test]
    fn write_csv_escapes_special_characters() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("export.csv");

        let payload = ExportPayload {
            version: "1.0".to_string(),
            vault_id: "test".to_string(),
            exported_at: "2026-05-05T00:00:00Z".to_string(),
            records: vec![ExportRecord {
                id: "1".to_string(),
                credential_type: "login".to_string(),
                name: "Entry, with comma".to_string(),
                username: Some("user\nmultiline".to_string()),
                password: Some("pass\"quotes".to_string()),
                url: None,
                notes: None,
                totp: None,
                tags: Some(vec!["tag,one".to_string(), "tag;two".to_string()]),
                is_favorite: None,
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            }],
        };

        write_csv(&payload, &path).expect("write should succeed");
        let data = std::fs::read(&path);
        assert!(data.is_ok());
        let binding = data.unwrap();
        let text = String::from_utf8_lossy(&binding);
        assert!(
            text.contains("Entry, with comma"),
            "comma in name should be preserved"
        );
    }

    // -- Test 12: validate_export_csv_path accepts .csv --

    #[test]
    fn validate_export_csv_path_accepts_csv() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("export.csv");
        assert!(validate_export_csv_path(&path).is_ok());
    }

    // -- Test 13: validate_export_csv_path rejects non-.csv --

    #[test]
    fn validate_export_csv_path_rejects_non_csv() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("export.txt");
        let result = validate_export_csv_path(&path);
        assert!(result.is_err());
        assert!(
            matches!(
                result.unwrap_err(),
                ImportExportError::InvalidFormat(msg) if msg.contains(".csv")
            ),
            "expected InvalidFormat mentioning .csv"
        );
    }
}
