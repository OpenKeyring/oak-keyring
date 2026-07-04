//! Serialization round-trip tests for OKB export/import through encryption/decryption.
//!
//! These tests verify the OKB serialization layer:
//! 1. Create ExportRecord with type-specific fields
//! 2. Serialize and encrypt via encrypt_and_write_okb
//! 3. Parse via OkbParser (decrypts and parses the .okb file)
//! 4. Map via map_parsed_item
//! 5. Verify mapped fields contain the correct values for each type
//!
//! This validates that Issue 46's fix works at the serialization level:
//! SSH/API type-specific fields survive encrypt→decrypt→parse round-trip.
//!
//! Note: The full vault chain (decrypted_record_to_export → fields_to_payload)
//! is tested in unit tests within `src/executor/import_export.rs`.

use oak_keyring::commands::types::ImportSource;
use oak_keyring::services::import_export::export::{
    encrypt_and_write_okb, ExportPayload, ExportRecord,
};
use oak_keyring::services::import_export::mapping::{infer_credential_type, map_parsed_item};
use oak_keyring::services::import_export::parser::FormatParser;
use oak_keyring::services::import_export::parsers::okb::OkbParser;
use oak_keyring::types::{CredentialType, SecureStr};
use tempfile::tempdir;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Build an ExportRecord for Login credentials.
fn build_login(id: &str, name: &str, username: &str, password: &str) -> ExportRecord {
    ExportRecord {
        id: id.to_string(),
        credential_type: "login".to_string(),
        name: name.to_string(),
        username: Some(username.to_string()),
        password: Some(password.to_string()),
        url: Some("https://example.com".to_string()),
        notes: Some("Login notes".to_string()),
        totp: Some("otpauth://totp/GitHub:alice?secret=JBSWY3DPEHPK3PXP&issuer=GitHub".to_string()),
        tags: Some(vec!["work".to_string(), "important".to_string()]),
        is_favorite: Some(true),
        expires_at: None,
        public_key: None,
        private_key: None,
        passphrase: None,
        app_id: None,
        secret_key: None,
    }
}

/// Build an ExportRecord for API credentials.
fn build_api(id: &str, name: &str, app_id: &str, secret_key: &str) -> ExportRecord {
    ExportRecord {
        id: id.to_string(),
        credential_type: "api".to_string(),
        name: name.to_string(),
        username: None,
        password: None,
        url: Some("https://api.example.com".to_string()),
        notes: Some("API credentials".to_string()),
        totp: None,
        tags: Some(vec!["dev".to_string()]),
        is_favorite: Some(false),
        expires_at: None,
        public_key: None,
        private_key: None,
        passphrase: None,
        app_id: Some(app_id.to_string()),
        secret_key: Some(secret_key.to_string()),
    }
}

/// Build an ExportRecord for SSH credentials with passphrase.
fn build_ssh_with_passphrase(
    id: &str,
    name: &str,
    public_key: &str,
    private_key: &str,
    passphrase: &str,
) -> ExportRecord {
    ExportRecord {
        id: id.to_string(),
        credential_type: "ssh".to_string(),
        name: name.to_string(),
        username: Some("deploy".to_string()),
        password: None,
        url: Some("ssh://example.com".to_string()),
        notes: Some("SSH key for deployment".to_string()),
        totp: None,
        tags: Some(vec!["servers".to_string(), "production".to_string()]),
        is_favorite: Some(true),
        expires_at: None,
        public_key: Some(public_key.to_string()),
        private_key: Some(private_key.to_string()),
        passphrase: Some(passphrase.to_string()),
        app_id: None,
        secret_key: None,
    }
}

/// Build an ExportRecord for SSH credentials without passphrase.
fn build_ssh_without_passphrase(
    id: &str,
    name: &str,
    public_key: &str,
    private_key: &str,
) -> ExportRecord {
    ExportRecord {
        id: id.to_string(),
        credential_type: "ssh".to_string(),
        name: name.to_string(),
        username: Some("git".to_string()),
        password: None,
        url: Some("ssh://github.com".to_string()),
        notes: Some("GitHub SSH key".to_string()),
        totp: None,
        tags: Some(vec!["git".to_string()]),
        is_favorite: Some(false),
        expires_at: None,
        public_key: Some(public_key.to_string()),
        private_key: Some(private_key.to_string()),
        passphrase: None,
        app_id: None,
        secret_key: None,
    }
}

/// Perform OKB serialization round-trip: serialize → encrypt → decrypt → parse → map.
///
/// Returns the parsed items and corresponding original records for verification.
fn serde_roundtrip_via_okb<'a>(
    records: &'a [ExportRecord],
    password: &str,
) -> Vec<(
    &'a ExportRecord,
    oak_keyring::services::import_export::types::MappedRecord,
)> {
    let payload = ExportPayload {
        version: "1".to_string(),
        vault_id: Uuid::new_v4().to_string(),
        exported_at: chrono::Utc::now().to_rfc3339(),
        records: records
            .iter()
            .map(|r| {
                // Reconstruct without Clone — copy individual fields.
                ExportRecord {
                    id: r.id.clone(),
                    credential_type: r.credential_type.clone(),
                    name: r.name.clone(),
                    username: r.username.clone(),
                    password: r.password.clone(),
                    url: r.url.clone(),
                    notes: r.notes.clone(),
                    totp: r.totp.clone(),
                    tags: r.tags.clone(),
                    is_favorite: r.is_favorite,
                    expires_at: r.expires_at.clone(),
                    public_key: r.public_key.clone(),
                    private_key: r.private_key.clone(),
                    passphrase: r.passphrase.clone(),
                    app_id: r.app_id.clone(),
                    secret_key: r.secret_key.clone(),
                }
            })
            .collect(),
    };

    let dir = tempdir().expect("tempdir must succeed");
    let output_path = dir.path().join("test.okb");
    let secure_password = SecureStr::new(password.to_string());

    encrypt_and_write_okb(&payload, &secure_password, &output_path)
        .expect("encrypt_and_write_okb must succeed");

    let parser = OkbParser;
    let parsed_items = parser
        .parse(&output_path, Some(&secure_password), None)
        .expect("parse must succeed");

    parsed_items
        .iter()
        .zip(records.iter())
        .map(|(parsed, original)| {
            let mapped = map_parsed_item(parsed, ImportSource::OpenKeyringBackup);
            (original, mapped)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

#[test]
fn serde_login_roundtrip() {
    let original = build_login("rec-1", "GitHub", "alice", "s3cr3t_passw0rd!");
    let records = [original];

    let results = serde_roundtrip_via_okb(&records, "export_password_123");

    assert_eq!(results.len(), 1, "should have exactly 1 record");
    let (original_export, mapped) = &results[0];

    assert_eq!(
        infer_credential_type(&mapped.fields),
        CredentialType::Login,
        "should infer Login type"
    );
    assert_eq!(mapped.credential_type, CredentialType::Login);

    assert_eq!(mapped.fields.get("name").unwrap(), &original_export.name);
    assert_eq!(
        mapped.fields.get("username").unwrap(),
        original_export.username.as_ref().unwrap()
    );
    assert_eq!(
        mapped.fields.get("password").unwrap(),
        original_export.password.as_ref().unwrap()
    );
    assert_eq!(
        mapped.fields.get("url").unwrap(),
        original_export.url.as_ref().unwrap()
    );
    assert_eq!(
        mapped.fields.get("notes").unwrap(),
        original_export.notes.as_ref().unwrap()
    );
    assert_eq!(
        mapped.fields.get("totp").unwrap(),
        original_export.totp.as_ref().unwrap()
    );
    assert_eq!(
        mapped.tags,
        original_export.tags.as_ref().unwrap().as_slice()
    );
    assert_eq!(
        mapped.fields.get("is_favorite").unwrap(),
        &original_export.is_favorite.unwrap().to_string()
    );
}

#[test]
fn serde_api_roundtrip() {
    let original = build_api(
        "rec-2",
        "AWS API",
        "AKIAIOSFODNN7EXAMPLE",
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    );

    let records = [original];
    let results = serde_roundtrip_via_okb(&records, "export_password_456");

    assert_eq!(results.len(), 1, "should have exactly 1 record");
    let (original_export, mapped) = &results[0];

    assert_eq!(
        infer_credential_type(&mapped.fields),
        CredentialType::Api,
        "should infer API type"
    );
    assert_eq!(mapped.credential_type, CredentialType::Api);

    assert_eq!(mapped.fields.get("name").unwrap(), &original_export.name);
    assert_eq!(
        mapped.fields.get("app_id").unwrap(),
        original_export.app_id.as_ref().unwrap(),
        "app_id must survive round-trip"
    );
    assert_eq!(
        mapped.fields.get("secret_key").unwrap(),
        original_export.secret_key.as_ref().unwrap(),
        "secret_key must survive round-trip"
    );
    assert_eq!(
        mapped.fields.get("url").unwrap(),
        original_export.url.as_ref().unwrap()
    );
    assert_eq!(
        mapped.fields.get("notes").unwrap(),
        original_export.notes.as_ref().unwrap()
    );
    assert_eq!(
        mapped.tags,
        original_export.tags.as_ref().unwrap().as_slice()
    );
    assert_eq!(
        mapped.fields.get("is_favorite").unwrap(),
        &original_export.is_favorite.unwrap().to_string()
    );
}

#[test]
fn serde_ssh_with_passphrase_roundtrip() {
    let original = build_ssh_with_passphrase(
        "rec-3",
        "Production Server",
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExamplePublicKey alice@deploy",
        "-----BEGIN OPENSSH PRIVATE KEY-----\n-----END OPENSSH PRIVATE KEY-----",
        "key_passphrase",
    );

    let records = [original];
    let results = serde_roundtrip_via_okb(&records, "export_password_789");

    assert_eq!(results.len(), 1, "should have exactly 1 record");
    let (original_export, mapped) = &results[0];

    assert_eq!(
        infer_credential_type(&mapped.fields),
        CredentialType::Ssh,
        "should infer SSH type"
    );
    assert_eq!(mapped.credential_type, CredentialType::Ssh);

    assert_eq!(mapped.fields.get("name").unwrap(), &original_export.name);
    assert_eq!(
        mapped.fields.get("public_key").unwrap(),
        original_export.public_key.as_ref().unwrap(),
        "public_key must survive round-trip"
    );
    assert_eq!(
        mapped.fields.get("private_key").unwrap(),
        original_export.private_key.as_ref().unwrap(),
        "private_key must survive round-trip"
    );
    assert_eq!(
        mapped.fields.get("passphrase").unwrap(),
        original_export.passphrase.as_ref().unwrap(),
        "passphrase must survive round-trip"
    );
    assert_eq!(
        mapped.fields.get("username").unwrap(),
        original_export.username.as_ref().unwrap()
    );
    assert_eq!(
        mapped.fields.get("url").unwrap(),
        original_export.url.as_ref().unwrap()
    );
    assert_eq!(
        mapped.fields.get("notes").unwrap(),
        original_export.notes.as_ref().unwrap()
    );
    assert_eq!(
        mapped.tags,
        original_export.tags.as_ref().unwrap().as_slice()
    );
    assert_eq!(
        mapped.fields.get("is_favorite").unwrap(),
        &original_export.is_favorite.unwrap().to_string()
    );
}

#[test]
fn serde_ssh_without_passphrase_roundtrip() {
    let original = build_ssh_without_passphrase(
        "rec-4",
        "GitHub",
        "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQC...",
        "-----BEGIN RSA PRIVATE KEY-----\n-----END RSA PRIVATE KEY-----",
    );

    let records = [original];
    let results = serde_roundtrip_via_okb(&records, "export_password_abc");

    assert_eq!(results.len(), 1, "should have exactly 1 record");
    let (original_export, mapped) = &results[0];

    assert_eq!(
        infer_credential_type(&mapped.fields),
        CredentialType::Ssh,
        "should infer SSH type"
    );
    assert_eq!(mapped.credential_type, CredentialType::Ssh);

    assert_eq!(mapped.fields.get("name").unwrap(), &original_export.name);
    assert_eq!(
        mapped.fields.get("public_key").unwrap(),
        original_export.public_key.as_ref().unwrap(),
        "public_key must survive round-trip"
    );
    assert_eq!(
        mapped.fields.get("private_key").unwrap(),
        original_export.private_key.as_ref().unwrap(),
        "private_key must survive round-trip"
    );
    assert!(
        !mapped.fields.contains_key("passphrase")
            || mapped.fields.get("passphrase").unwrap().is_empty(),
        "passphrase should be absent or empty when not set"
    );
    assert_eq!(
        mapped.tags,
        original_export.tags.as_ref().unwrap().as_slice()
    );
    assert_eq!(
        mapped.fields.get("is_favorite").unwrap(),
        &original_export.is_favorite.unwrap().to_string()
    );
}

#[test]
fn serde_mixed_types_roundtrip() {
    let records = vec![
        build_login("rec-1", "GitHub", "alice", "pass123"),
        build_api(
            "rec-2",
            "Stripe API",
            "sk_test_4eC39HqLyjWDarjtT1zdp7dc",
            "sk_live_51H...",
        ),
        build_ssh_with_passphrase(
            "rec-3",
            "Deploy Server",
            "ssh-ed25519 AAAAC3...",
            "-----BEGIN OPENSSH PRIVATE KEY-----\n...",
            "deploy_key_pass",
        ),
        build_ssh_without_passphrase(
            "rec-4",
            "GitLab",
            "ssh-rsa AAAAB3...",
            "-----BEGIN RSA PRIVATE KEY-----\n...",
        ),
    ];

    let results = serde_roundtrip_via_okb(&records, "mixed_export_password");

    assert_eq!(results.len(), 4, "should have all 4 records");

    for (original_export, mapped) in &results {
        let id = &original_export.id;

        let original = records
            .iter()
            .find(|r| r.id == *id)
            .expect("original record must exist");

        match original_export.credential_type.as_str() {
            "login" => {
                assert_eq!(mapped.credential_type, CredentialType::Login);
                assert_eq!(
                    mapped.fields.get("username").unwrap(),
                    original.username.as_ref().unwrap()
                );
                assert_eq!(
                    mapped.fields.get("password").unwrap(),
                    original.password.as_ref().unwrap()
                );
            }
            "api" => {
                assert_eq!(mapped.credential_type, CredentialType::Api);
                assert_eq!(
                    mapped.fields.get("app_id").unwrap(),
                    original.app_id.as_ref().unwrap(),
                    "API app_id must survive round-trip in mixed export"
                );
                assert_eq!(
                    mapped.fields.get("secret_key").unwrap(),
                    original.secret_key.as_ref().unwrap(),
                    "API secret_key must survive round-trip in mixed export"
                );
            }
            "ssh" => {
                assert_eq!(mapped.credential_type, CredentialType::Ssh);
                assert_eq!(
                    mapped.fields.get("public_key").unwrap(),
                    original.public_key.as_ref().unwrap(),
                    "SSH public_key must survive round-trip in mixed export"
                );
                assert_eq!(
                    mapped.fields.get("private_key").unwrap(),
                    original.private_key.as_ref().unwrap(),
                    "SSH private_key must survive round-trip in mixed export"
                );
                if let Some(passphrase) = &original.passphrase {
                    assert_eq!(
                        mapped.fields.get("passphrase").unwrap(),
                        passphrase,
                        "SSH passphrase must survive round-trip when present"
                    );
                }
            }
            _ => panic!(
                "unknown credential type: {}",
                original_export.credential_type
            ),
        }

        assert_eq!(mapped.fields.get("name").unwrap(), &original.name);
        assert_eq!(mapped.tags, original.tags.as_ref().unwrap().as_slice());
    }
}
