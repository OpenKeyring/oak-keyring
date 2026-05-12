//! OpenKeyring .okb backup file import parser.
//!
//! Parses encrypted backup files using Argon2id key derivation and
//! XChaCha20-Poly1305 authenticated decryption.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;
use zeroize::Zeroize;

use crate::commands::types::ImportSource;
use crate::crypto;
use crate::errors::mapping::import_export::ImportExportError;
use crate::services::import_export::parser::{validate_file_common, FormatParser, ParsedItem};
use crate::types::SecureStr;

// -- OKB binary format constants ------------------------------------------------

/// Expected format version (u32 little-endian).
const OKB_VERSION: u32 = 1;

/// Byte offset where the 16-byte salt begins.
const SALT_OFFSET: usize = 4;
/// Length of the Argon2id salt.
const SALT_LEN: usize = 16;

/// Byte offset where the 24-byte nonce begins.
const NONCE_OFFSET: usize = SALT_OFFSET + SALT_LEN;
/// Length of the XChaCha20-Poly1305 nonce.
const NONCE_LEN: usize = 24;

/// Byte offset where the ciphertext (including 16-byte Poly1305 tag) begins.
const CIPHERTEXT_OFFSET: usize = NONCE_OFFSET + NONCE_LEN;

// -- JSON payload types ---------------------------------------------------------

#[derive(Deserialize)]
struct OkbPayload {
    #[allow(dead_code)]
    version: String,
    #[allow(dead_code)]
    vault_id: String,
    #[allow(dead_code)]
    exported_at: String,
    records: Vec<OkbRecord>,
}

#[derive(Deserialize)]
struct OkbRecord {
    id: String,
    credential_type: String,
    name: String,
    username: Option<String>,
    password: Option<String>,
    url: Option<String>,
    notes: Option<String>,
    tags: Option<Vec<String>>,
    is_favorite: Option<bool>,
    expires_at: Option<String>,
    // Type-specific fields (Issue 46)
    public_key: Option<String>,
    private_key: Option<String>,
    passphrase: Option<String>,
    app_id: Option<String>,
    secret_key: Option<String>,
}

// -- OkbParser ------------------------------------------------------------------

pub struct OkbParser;

impl FormatParser for OkbParser {
    fn format(&self) -> ImportSource {
        ImportSource::OpenKeyringBackup
    }

    fn parse(
        &self,
        path: &Path,
        password: Option<&SecureStr>,
        _csv_mapping: Option<&crate::commands::types::CsvColumnMapping>,
    ) -> Result<Vec<ParsedItem>, ImportExportError> {
        // Read the entire file.
        let data = std::fs::read(path).map_err(|e| ImportExportError::FileReadError {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

        // Minimum size: 4 (version) + 16 (salt) + 24 (nonce) + 16 (Poly1305 tag) = 60 bytes.
        if data.len() < CIPHERTEXT_OFFSET + 16 {
            return Err(ImportExportError::InvalidFormat(
                "file too short to be a valid .okb backup".into(),
            ));
        }

        // 1. Parse version (u32 little-endian).
        let version = u32::from_le_bytes(
            data[..SALT_OFFSET]
                .try_into()
                .expect("SALT_OFFSET == 4, always fits in [u8; 4]"),
        );
        if version != OKB_VERSION {
            return Err(ImportExportError::InvalidFormat(format!(
                "unsupported .okb version {version}, expected {OKB_VERSION}"
            )));
        }

        // 2. Extract salt.
        let salt: [u8; SALT_LEN] = data[SALT_OFFSET..NONCE_OFFSET]
            .try_into()
            .expect("salt slice is exactly SALT_LEN bytes");

        // 3. Extract nonce.
        let nonce: [u8; NONCE_LEN] = data[NONCE_OFFSET..CIPHERTEXT_OFFSET]
            .try_into()
            .expect("nonce slice is exactly NONCE_LEN bytes");

        // 4. Extract ciphertext.
        let ciphertext = &data[CIPHERTEXT_OFFSET..];

        // 5. Password is required.
        let password = password.ok_or(ImportExportError::PasswordRequired)?;

        // 6. Derive DEK via Argon2id.
        let dek =
            crypto::argon2::derive_key_locked(password, &salt, &crypto::argon2::Argon2Params::medium())
                .map_err(ImportExportError::DecryptionFailed)?;

        // 7. Decrypt with XChaCha20-Poly1305.
        let mut plaintext = crypto::xchacha20::decrypt(ciphertext, &nonce, dek.expose())
            .map_err(ImportExportError::DecryptionFailed)?;

        // 8. Deserialize JSON payload.
        let payload: OkbPayload =
            serde_json::from_slice(&plaintext).map_err(|e| ImportExportError::ParseError {
                format: "okb".into(),
                reason: e.to_string(),
            })?;
        plaintext.zeroize();

        // 9. Convert records to ParsedItems.
        let items = payload
            .records
            .into_iter()
            .map(|r| {
                let mut fields = HashMap::new();
                fields.insert("name".into(), r.name);
                fields.insert("credential_type".into(), r.credential_type);
                insert_optional(&mut fields, "username", r.username);
                insert_optional(&mut fields, "password", r.password);
                insert_optional(&mut fields, "url", r.url);
                insert_optional(&mut fields, "notes", r.notes);
                if let Some(fav) = r.is_favorite {
                    fields.insert("is_favorite".into(), fav.to_string());
                }
                insert_optional(&mut fields, "expires_at", r.expires_at);
                // Type-specific fields (Issue 46)
                insert_optional(&mut fields, "public_key", r.public_key);
                insert_optional(&mut fields, "private_key", r.private_key);
                insert_optional(&mut fields, "passphrase", r.passphrase);
                insert_optional(&mut fields, "app_id", r.app_id);
                insert_optional(&mut fields, "secret_key", r.secret_key);

                ParsedItem {
                    source_id: r.id,
                    fields,
                    tags: r.tags.unwrap_or_default(),
                }
            })
            .collect();

        Ok(items)
    }

    fn requires_password(&self) -> bool {
        true
    }

    fn validate_file(&self, path: &Path) -> Result<(), ImportExportError> {
        validate_file_common(path, "okb")
    }
}

/// Insert an optional value into the fields map only when it is `Some`.
fn insert_optional(fields: &mut HashMap<String, String>, key: &str, value: Option<String>) {
    if let Some(v) = value {
        fields.insert(key.to_string(), v);
    }
}

// -- Tests ----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SecureStr;

    /// Build a valid .okb file in-memory from the given JSON payload and password.
    fn build_okb(payload: &str, password: &str) -> Vec<u8> {
        let salt = crypto::argon2::generate_salt();
        let nonce_raw = crypto::xchacha20::encrypt(payload.as_bytes(), &{
            // We need a 32-byte key first; derive it.
            let key_vec = crypto::argon2::derive_key(password, &salt).expect("derive_key");
            let key: [u8; 32] = key_vec.try_into().unwrap();
            key
        })
        .expect("encrypt");

        // Build the file bytes.
        let mut buf = Vec::with_capacity(4 + SALT_LEN + NONCE_LEN + nonce_raw.0.len());
        buf.extend_from_slice(&OKB_VERSION.to_le_bytes());
        buf.extend_from_slice(&salt);
        buf.extend_from_slice(&nonce_raw.1);
        buf.extend_from_slice(&nonce_raw.0);
        buf
    }

    /// Write `data` to a temp file with the given name and return the path.
    fn write_temp(name: &str, data: &[u8]) -> tempfile::NamedTempFile {
        let tf = tempfile::Builder::new()
            .suffix(name)
            .rand_bytes(6)
            .tempfile()
            .expect("create temp file");
        std::fs::write(tf.path(), data).expect("write temp");
        tf
    }

    fn test_payload() -> String {
        r#"{
            "version": "1.0",
            "vault_id": "550e8400-e29b-41d4-a716-446655440000",
            "exported_at": "2024-01-01T00:00:00Z",
            "records": [
                {
                    "id": "11111111-1111-1111-1111-111111111111",
                    "credential_type": "login",
                    "name": "Example",
                    "username": "user@example.com",
                    "password": "s3cret",
                    "url": "https://example.com",
                    "notes": "test notes",
                    "tags": ["tag1", "tag2"],
                    "is_favorite": true,
                    "expires_at": null
                }
            ]
        }"#
        .into()
    }

    fn multi_record_payload() -> String {
        r#"{
            "version": "1.0",
            "vault_id": "550e8400-e29b-41d4-a716-446655440000",
            "exported_at": "2024-01-01T00:00:00Z",
            "records": [
                {
                    "id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                    "credential_type": "login",
                    "name": "Gmail",
                    "username": "alice@gmail.com",
                    "password": "pw1",
                    "url": "https://gmail.com",
                    "notes": null,
                    "tags": ["email"],
                    "is_favorite": false,
                    "expires_at": null
                },
                {
                    "id": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
                    "credential_type": "api",
                    "name": "AWS Key",
                    "username": null,
                    "password": null,
                    "url": null,
                    "notes": "production key",
                    "tags": ["cloud", "aws"],
                    "is_favorite": true,
                    "expires_at": "2025-12-31T23:59:59Z",
                    "app_id": "AKIA123",
                    "secret_key": "secret-key-123"
                },
                {
                    "id": "cccccccc-cccc-cccc-cccc-cccccccccccc",
                    "credential_type": "ssh",
                    "name": "GitHub SSH",
                    "username": null,
                    "password": null,
                    "url": null,
                    "notes": "ed25519 key",
                    "tags": [],
                    "is_favorite": null,
                    "expires_at": null,
                    "public_key": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEXAMPLE",
                    "private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\nEXAMPLE\n-----END OPENSSH PRIVATE KEY-----",
                    "passphrase": null
                }
            ]
        }"#
        .into()
    }

    // -- Core tests --------------------------------------------------------------

    #[test]
    fn decrypt_okb_returns_correct_parsed_items() {
        let payload = test_payload();
        let data = build_okb(&payload, "test-password");
        let tf = write_temp("backup.okb", &data);

        let parser = OkbParser;
        let pw = SecureStr::new("test-password".into());
        let items = parser
            .parse(tf.path(), Some(&pw), None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.source_id, "11111111-1111-1111-1111-111111111111");
        assert_eq!(item.fields.get("name").unwrap(), "Example");
        assert_eq!(item.fields.get("username").unwrap(), "user@example.com");
        assert_eq!(item.fields.get("password").unwrap(), "s3cret");
        assert_eq!(item.fields.get("url").unwrap(), "https://example.com");
        assert_eq!(item.fields.get("notes").unwrap(), "test notes");
        assert_eq!(item.fields.get("credential_type").unwrap(), "login");
        assert_eq!(item.fields.get("is_favorite").unwrap(), "true");
        assert_eq!(item.tags, vec!["tag1", "tag2"]);
    }

    #[test]
    fn multiple_records_produces_correct_count_and_fields() {
        let payload = multi_record_payload();
        let data = build_okb(&payload, "multi-pw");
        let tf = write_temp("multi.okb", &data);

        let parser = OkbParser;
        let pw = SecureStr::new("multi-pw".into());
        let items = parser
            .parse(tf.path(), Some(&pw), None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 3);

        // Record 1 — login
        assert_eq!(items[0].source_id, "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
        assert_eq!(items[0].fields.get("name").unwrap(), "Gmail");
        assert_eq!(items[0].tags, vec!["email"]);

        // Record 2 — api
        assert_eq!(items[1].source_id, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
        assert_eq!(items[1].fields.get("credential_type").unwrap(), "api");
        assert_eq!(
            items[1].fields.get("expires_at").unwrap(),
            "2025-12-31T23:59:59Z"
        );
        assert_eq!(items[1].tags, vec!["cloud", "aws"]);
        // Verify type-specific fields (Issue 46)
        assert!(items[1].fields.contains_key("app_id"));
        assert_eq!(items[1].fields.get("app_id").unwrap(), "AKIA123");
        assert!(items[1].fields.contains_key("secret_key"));
        assert_eq!(items[1].fields.get("secret_key").unwrap(), "secret-key-123");

        // Record 3 — ssh
        assert_eq!(items[2].source_id, "cccccccc-cccc-cccc-cccc-cccccccccccc");
        assert_eq!(items[2].fields.get("credential_type").unwrap(), "ssh");
        assert!(!items[2].fields.contains_key("password"));
        assert!(items[2].tags.is_empty());
        // Verify type-specific fields (Issue 46)
        assert!(items[2].fields.contains_key("public_key"));
        assert!(items[2].fields.contains_key("private_key"));
    }

    #[test]
    fn no_password_returns_password_required_error() {
        let payload = test_payload();
        let data = build_okb(&payload, "any-pw");
        let tf = write_temp("nopw.okb", &data);

        let parser = OkbParser;
        let result = parser.parse(tf.path(), None, None);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ImportExportError::PasswordRequired),
            "expected PasswordRequired"
        );
    }

    #[test]
    fn wrong_password_returns_decryption_failed_error() {
        let payload = test_payload();
        let data = build_okb(&payload, "correct-password");
        let tf = write_temp("wrongpw.okb", &data);

        let parser = OkbParser;
        let pw = SecureStr::new("wrong-password".into());
        let result = parser.parse(tf.path(), Some(&pw), None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::DecryptionFailed(_)),
            "expected DecryptionFailed, got: {err:?}"
        );
    }

    #[test]
    fn invalid_version_returns_invalid_format_error() {
        let payload = test_payload();
        let mut data = build_okb(&payload, "pw");
        // Overwrite the version bytes with version=2.
        data[..4].copy_from_slice(&2u32.to_le_bytes());
        let tf = write_temp("v2.okb", &data);

        let parser = OkbParser;
        let pw = SecureStr::new("pw".into());
        let result = parser.parse(tf.path(), Some(&pw), None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::InvalidFormat(ref msg) if msg.contains("version")),
            "expected InvalidFormat mentioning version, got: {err:?}"
        );
    }

    #[test]
    fn corrupted_truncated_file_returns_error() {
        // Only 20 bytes — not enough for the full header + ciphertext.
        let data = vec![0u8; 20];
        let tf = write_temp("trunc.okb", &data);

        let parser = OkbParser;
        let pw = SecureStr::new("pw".into());
        let result = parser.parse(tf.path(), Some(&pw), None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::InvalidFormat(_)),
            "expected InvalidFormat for truncated file, got: {err:?}"
        );
    }

    #[test]
    fn empty_records_returns_empty_vec() {
        let payload = r#"{
            "version": "1.0",
            "vault_id": "550e8400-e29b-41d4-a716-446655440000",
            "exported_at": "2024-01-01T00:00:00Z",
            "records": []
        }"#;
        let data = build_okb(payload, "empty-pw");
        let tf = write_temp("empty.okb", &data);

        let parser = OkbParser;
        let pw = SecureStr::new("empty-pw".into());
        let items = parser
            .parse(tf.path(), Some(&pw), None)
            .expect("parse should succeed");
        assert!(items.is_empty());
    }

    #[test]
    fn validate_file_okb_extension_returns_ok() {
        let payload = test_payload();
        let data = build_okb(&payload, "pw");
        let tf = write_temp("data.okb", &data);

        let parser = OkbParser;
        assert!(parser.validate_file(tf.path()).is_ok());
    }

    #[test]
    fn validate_file_wrong_extension_returns_error() {
        let payload = test_payload();
        let data = build_okb(&payload, "pw");
        let tf = write_temp("data.txt", &data);

        let parser = OkbParser;
        let result = parser.validate_file(tf.path());
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ImportExportError::InvalidFormat(_)),
            "expected InvalidFormat"
        );
    }

    #[test]
    fn requires_password_returns_true() {
        let parser = OkbParser;
        assert!(parser.requires_password());
    }

    #[test]
    fn format_returns_open_keyring_backup() {
        let parser = OkbParser;
        assert_eq!(parser.format(), ImportSource::OpenKeyringBackup);
    }
}
