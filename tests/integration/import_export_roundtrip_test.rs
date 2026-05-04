//! End-to-end tests for export-import round-trip through encryption/decryption.
//!
//! These tests verify the COMPLETE data path:
//! 1. Create ExportRecord with type-specific fields
//! 2. Serialize and encrypt via encrypt_and_write_okb
//! 3. Parse via OkbParser (decrypts and parses the .okb file)
//! 4. Map via map_parsed_item
//! 5. Verify mapped fields contain the correct values for each type
//!
//! This validates that Issue 46's fix works end-to-end: SSH/API credentials
//! survive export→encrypt→decrypt→import round-trip.

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
fn build_export_record_login(id: &str, name: &str, username: &str, password: &str) -> ExportRecord {
    ExportRecord {
        id: id.to_string(),
        credential_type: "login".to_string(),
        name: name.to_string(),
        username: Some(username.to_string()),
        password: Some(password.to_string()),
        url: Some("https://example.com".to_string()),
        notes: Some("Login notes".to_string()),
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
fn build_export_record_api(id: &str, name: &str, app_id: &str, secret_key: &str) -> ExportRecord {
    ExportRecord {
        id: id.to_string(),
        credential_type: "api".to_string(),
        name: name.to_string(),
        username: None,
        password: None,
        url: Some("https://api.example.com".to_string()),
        notes: Some("API credentials".to_string()),
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
fn build_export_record_ssh_with_passphrase(
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
fn build_export_record_ssh_without_passphrase(
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

/// Perform a full round-trip: encrypt → write → parse → map.
///
/// Returns the mapped records for verification.
fn roundtrip_via_okb(
    records: Vec<ExportRecord>,
    password: &str,
) -> Vec<(
    ExportRecord,
    oak_keyring::services::import_export::types::MappedRecord,
)> {
    // 1. Build ExportPayload
    let payload = ExportPayload {
        version: "1".to_string(),
        vault_id: Uuid::new_v4().to_string(),
        exported_at: chrono::Utc::now().to_rfc3339(),
        records: records.clone(),
    };

    // 2. Write encrypted .okb file
    let dir = tempdir().expect("tempdir must succeed");
    let output_path = dir.path().join("test.okb");
    let secure_password = SecureStr::new(password.to_string());

    encrypt_and_write_okb(&payload, &secure_password, &output_path)
        .expect("encrypt_and_write_okb must succeed");

    // 3. Parse the .okb file (decrypts and parses)
    let parser = OkbParser;
    let parsed_items = parser
        .parse(&output_path, Some(&secure_password), None)
        .expect("parse must succeed");

    // 4. Map each parsed item
    let mapped_records: Vec<_> = parsed_items
        .iter()
        .zip(records.iter())
        .map(|(parsed, original)| {
            let mapped = map_parsed_item(parsed, ImportSource::OpenKeyringBackup);
            (original.clone(), mapped)
        })
        .collect();

    mapped_records
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

#[test]
fn e2e_login_credential_roundtrip() {
    let original = build_export_record_login("rec-1", "GitHub", "alice", "s3cr3t_passw0rd!");

    let results = roundtrip_via_okb(vec![original.clone()], "export_password_123");

    assert_eq!(results.len(), 1, "should have exactly 1 record");
    let (original_export, mapped) = &results[0];

    // Verify credential type inference
    assert_eq!(
        infer_credential_type(&mapped.fields),
        CredentialType::Login,
        "should infer Login type"
    );
    assert_eq!(mapped.credential_type, CredentialType::Login);

    // Verify Login fields
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

    // Verify tags
    assert_eq!(
        mapped.tags,
        original_export.tags.as_ref().unwrap().as_slice()
    );

    // Verify favorite
    assert_eq!(
        mapped.fields.get("is_favorite").unwrap(),
        &original_export.is_favorite.unwrap().to_string()
    );
}

#[test]
fn e2e_api_credential_roundtrip() {
    let original = build_export_record_api(
        "rec-2",
        "AWS API",
        "AKIAIOSFODNN7EXAMPLE",
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    );

    let results = roundtrip_via_okb(vec![original.clone()], "export_password_456");

    assert_eq!(results.len(), 1, "should have exactly 1 record");
    let (original_export, mapped) = &results[0];

    // Verify credential type inference
    assert_eq!(
        infer_credential_type(&mapped.fields),
        CredentialType::Api,
        "should infer API type"
    );
    assert_eq!(mapped.credential_type, CredentialType::Api);

    // Verify API fields (Issue 46 fix)
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

    // Verify tags
    assert_eq!(
        mapped.tags,
        original_export.tags.as_ref().unwrap().as_slice()
    );

    // Verify favorite
    assert_eq!(
        mapped.fields.get("is_favorite").unwrap(),
        &original_export.is_favorite.unwrap().to_string()
    );
}

#[test]
fn e2e_ssh_credential_with_passphrase_roundtrip() {
    let original = build_export_record_ssh_with_passphrase(
        "rec-3",
        "Production Server",
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExamplePublicKey alice@deploy",
        "-----BEGIN OPENSSH PRIVATE KEY-----\n-----END OPENSSH PRIVATE KEY-----",
        "key_passphrase",
    );

    let results = roundtrip_via_okb(vec![original.clone()], "export_password_789");

    assert_eq!(results.len(), 1, "should have exactly 1 record");
    let (original_export, mapped) = &results[0];

    // Verify credential type inference
    assert_eq!(
        infer_credential_type(&mapped.fields),
        CredentialType::Ssh,
        "should infer SSH type"
    );
    assert_eq!(mapped.credential_type, CredentialType::Ssh);

    // Verify SSH fields (Issue 46 fix)
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

    // Verify tags
    assert_eq!(
        mapped.tags,
        original_export.tags.as_ref().unwrap().as_slice()
    );

    // Verify favorite
    assert_eq!(
        mapped.fields.get("is_favorite").unwrap(),
        &original_export.is_favorite.unwrap().to_string()
    );
}

#[test]
fn e2e_ssh_credential_without_passphrase_roundtrip() {
    let original = build_export_record_ssh_without_passphrase(
        "rec-4",
        "GitHub",
        "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQC...",
        "-----BEGIN RSA PRIVATE KEY-----\n-----END RSA PRIVATE KEY-----",
    );

    let results = roundtrip_via_okb(vec![original.clone()], "export_password_abc");

    assert_eq!(results.len(), 1, "should have exactly 1 record");
    let (original_export, mapped) = &results[0];

    // Verify credential type inference
    assert_eq!(
        infer_credential_type(&mapped.fields),
        CredentialType::Ssh,
        "should infer SSH type"
    );
    assert_eq!(mapped.credential_type, CredentialType::Ssh);

    // Verify SSH fields without passphrase (Issue 46 fix)
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

    // Verify tags
    assert_eq!(
        mapped.tags,
        original_export.tags.as_ref().unwrap().as_slice()
    );

    // Verify favorite
    assert_eq!(
        mapped.fields.get("is_favorite").unwrap(),
        &original_export.is_favorite.unwrap().to_string()
    );
}

#[test]
fn e2e_mixed_credential_types_roundtrip() {
    let records = vec![
        build_export_record_login("rec-1", "GitHub", "alice", "pass123"),
        build_export_record_api(
            "rec-2",
            "Stripe API",
            "sk_test_4eC39HqLyjWDarjtT1zdp7dc",
            "sk_live_51H...", // Truncated for brevity
        ),
        build_export_record_ssh_with_passphrase(
            "rec-3",
            "Deploy Server",
            "ssh-ed25519 AAAAC3...",
            "-----BEGIN OPENSSH PRIVATE KEY-----\n...",
            "deploy_key_pass",
        ),
        build_export_record_ssh_without_passphrase(
            "rec-4",
            "GitLab",
            "ssh-rsa AAAAB3...",
            "-----BEGIN RSA PRIVATE KEY-----\n...",
        ),
    ];

    let results = roundtrip_via_okb(records.clone(), "mixed_export_password");

    assert_eq!(results.len(), 4, "should have all 4 records");

    // Verify each record
    for (original_export, mapped) in results {
        let id = &original_export.id;

        // Find the original record by ID
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

        // Verify common fields
        assert_eq!(mapped.fields.get("name").unwrap(), &original.name);
        assert_eq!(mapped.tags, original.tags.as_ref().unwrap().as_slice());
    }
}
