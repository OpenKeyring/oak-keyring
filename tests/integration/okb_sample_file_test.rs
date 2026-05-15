//! Integration tests for OKB sample file parsing.
//!
//! These tests verify the OKB parser's ability to handle various file formats:
//! - Valid OKB files with different credential types
//! - Edge cases (special characters, long text, Unicode)
//! - Error cases (corrupted headers, wrong versions, truncated files, wrong passwords)
//!
//! Full pipeline integration tests verify the complete import flow:
//! parse → map → create in Vault → verify records

use std::collections::HashSet;

use oak_keyring::commands::types::{ImportSource, RecordFilter, RecordSort};
use oak_keyring::crypto::bip39::{MnemonicLanguage, Passkey};
use oak_keyring::db::schema::init_db_in_memory;
use oak_keyring::services::import_export::duplicate::ExistingRecordKey;
use oak_keyring::services::import_export::parser::{FormatParser, ParsedItem};
use oak_keyring::services::import_export::parsers::okb::OkbParser;
use oak_keyring::services::import_export::ImportExportService;
use oak_keyring::services::vault::VaultService;
use oak_keyring::types::credential::EncryptedPayload;
use oak_keyring::types::record::CreateRecordParams;
use oak_keyring::types::{CredentialType, SecureStr};

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Parse an OKB file with the correct test password.
fn parse_okb(name: &str) -> Vec<ParsedItem> {
    let path = std::path::Path::new("tests/data").join(name);
    if !path.exists() {
        eprintln!("Skipping: {} not found", name);
        return Vec::new();
    }
    let pw = SecureStr::new("test-password".to_string());
    let parser = OkbParser;
    parser
        .parse(&path, Some(&pw), None)
        .unwrap_or_else(|e| panic!("failed to parse {}: {}", name, e))
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

#[test]
fn test_okb_basic_parse() {
    let items = parse_okb("okb_basic.okb");
    if items.is_empty() {
        return; // File not found
    }

    assert_eq!(items.len(), 3, "should parse 3 login records");

    // Record 0: GitHub
    let github = &items[0];
    assert_eq!(
        github.fields.get("credential_type"),
        Some(&"login".to_string())
    );
    assert_eq!(github.fields.get("name"), Some(&"GitHub".to_string()));
    assert_eq!(
        github.fields.get("username"),
        Some(&"developer".to_string())
    );
    assert_eq!(
        github.fields.get("password"),
        Some(&"gh_secret_123".to_string())
    );
    assert!(github.fields.get("url").unwrap().contains("github.com"));
    assert_eq!(github.tags, vec!["dev"]);
    assert_eq!(github.fields.get("is_favorite"), Some(&"true".to_string()));

    // Record 1: Gmail
    let gmail = &items[1];
    assert_eq!(
        gmail.fields.get("credential_type"),
        Some(&"login".to_string())
    );
    assert_eq!(gmail.fields.get("name"), Some(&"Gmail".to_string()));
    assert_eq!(
        gmail.fields.get("username"),
        Some(&"user@gmail.com".to_string())
    );
    assert_eq!(
        gmail.fields.get("password"),
        Some(&"gm_pass_456".to_string())
    );
    assert!(gmail.tags.contains(&"email".to_string()));
    assert!(gmail.tags.contains(&"work".to_string()));

    // Record 2: AWS Console
    let aws = &items[2];
    assert_eq!(
        aws.fields.get("credential_type"),
        Some(&"login".to_string())
    );
    assert_eq!(aws.fields.get("name"), Some(&"AWS Console".to_string()));
    assert_eq!(
        aws.fields.get("username"),
        Some(&"admin@company".to_string())
    );
    assert_eq!(
        aws.fields.get("password"),
        Some(&"aws_root_789".to_string())
    );
    assert_eq!(aws.tags, vec!["cloud", "infra"]);
}

#[test]
fn test_okb_mixed_types_parse() {
    let items = parse_okb("okb_mixed_types.okb");
    if items.is_empty() {
        return; // File not found
    }

    assert_eq!(items.len(), 3, "should parse 3 records of different types");

    // Find GitLab (login)
    let gitlab = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"GitLab".to_string()))
        .expect("GitLab record should exist");
    assert_eq!(
        gitlab.fields.get("credential_type"),
        Some(&"login".to_string())
    );
    assert_eq!(
        gitlab.fields.get("username"),
        Some(&"dev@gitlab".to_string())
    );
    assert_eq!(gitlab.fields.get("password"), Some(&"gl_pass".to_string()));

    // Find AWS API Key (api)
    let aws_api = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"AWS API Key".to_string()))
        .expect("AWS API Key record should exist");
    assert_eq!(
        aws_api.fields.get("credential_type"),
        Some(&"api".to_string())
    );
    assert_eq!(
        aws_api.fields.get("app_id"),
        Some(&"AKIAIOSFODNN7".to_string())
    );
    assert_eq!(
        aws_api.fields.get("secret_key"),
        Some(&"wJalrXUtnFEMI/K7MDENG".to_string())
    );

    // Find GitHub SSH (ssh)
    let github_ssh = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"GitHub SSH".to_string()))
        .expect("GitHub SSH record should exist");
    assert_eq!(
        github_ssh.fields.get("credential_type"),
        Some(&"ssh".to_string())
    );
    assert!(github_ssh.fields.contains_key("public_key"));
    assert!(github_ssh.fields.contains_key("private_key"));
    assert!(github_ssh.fields.contains_key("passphrase"));
}

#[test]
fn test_okb_edge_cases_parse() {
    let items = parse_okb("okb_edge_cases.okb");
    if items.is_empty() {
        return; // File not found
    }

    assert_eq!(items.len(), 4, "should parse 4 edge case records");

    // Record with special characters in name
    let special_name = items
        .iter()
        .find(|item| {
            item.fields
                .get("name")
                .map(|n: &String| n.contains(',') && n.contains('"'))
                .unwrap_or(false)
        })
        .expect("Record with comma and quote in name should exist");
    assert!(special_name
        .fields
        .get("password")
        .unwrap()
        .contains::<&str>("<>&\"'"));
    assert!(special_name.fields.get("notes").unwrap().contains("line1"));

    // Record with long notes
    let long_notes = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"Long Notes".to_string()))
        .expect("Long Notes record should exist");
    assert!(
        long_notes.fields.get("notes").unwrap().len() > 2000,
        "notes field should be very long"
    );

    // Record with Chinese characters
    let chinese = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"测试账户".to_string()))
        .expect("Chinese record should exist");
    assert!(chinese.fields.get("username").unwrap().contains("用户"));
    assert!(chinese.tags.iter().any(|t: &String| t.contains("标签")));

    // Minimal entry (only required fields)
    let minimal = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"Minimal Entry".to_string()))
        .expect("Minimal Entry record should exist");
    assert_eq!(
        minimal.fields.get("credential_type"),
        Some(&"login".to_string())
    );
    // These fields should be absent or empty in a minimal record
    assert!(
        minimal
            .fields
            .get("username")
            .map(|u: &String| u.is_empty())
            .unwrap_or(true),
        "username should be absent or empty in minimal record"
    );
    assert!(
        minimal
            .fields
            .get("password")
            .map(|p: &String| p.is_empty())
            .unwrap_or(true),
        "password should be absent or empty in minimal record"
    );
    assert!(
        minimal
            .fields
            .get("url")
            .map(|u: &String| u.is_empty())
            .unwrap_or(true),
        "url should be absent or empty in minimal record"
    );
}

#[test]
fn test_okb_corrupted_header_rejected() {
    let path = std::path::Path::new("tests/data").join("okb_corrupted_header.bin");
    if !path.exists() {
        eprintln!("Skipping: okb_corrupted_header.bin not found");
        return;
    }

    let pw = SecureStr::new("test-password".to_string());
    let parser = OkbParser;
    let result = parser.parse(&path, Some(&pw), None);

    assert!(result.is_err(), "corrupted header file should be rejected");

    let err = result.unwrap_err();
    let err_msg = err.to_string().to_lowercase();

    // Error should be either InvalidFormat or DecryptionFailed
    assert!(
        err_msg.contains("invalid format")
            || err_msg.contains("decryption")
            || err_msg.contains("version"),
        "error should mention format, decryption, or version issue, got: {}",
        err
    );
}

#[test]
fn test_okb_wrong_version_rejected() {
    let path = std::path::Path::new("tests/data").join("okb_wrong_version.bin");
    if !path.exists() {
        eprintln!("Skipping: okb_wrong_version.bin not found");
        return;
    }

    let pw = SecureStr::new("test-password".to_string());
    let parser = OkbParser;
    let result = parser.parse(&path, Some(&pw), None);

    assert!(result.is_err(), "wrong version file should be rejected");

    let err = result.unwrap_err();
    let err_msg = err.to_string().to_lowercase();

    assert!(
        err_msg.contains("invalid format")
            || err_msg.contains("version")
            || err_msg.contains("unsupported"),
        "error should mention format or version issue, got: {}",
        err
    );
}

#[test]
fn test_okb_truncated_rejected() {
    let path = std::path::Path::new("tests/data").join("okb_truncated.bin");
    if !path.exists() {
        eprintln!("Skipping: okb_truncated.bin not found");
        return;
    }

    let pw = SecureStr::new("test-password".to_string());
    let parser = OkbParser;
    let result = parser.parse(&path, Some(&pw), None);

    assert!(result.is_err(), "truncated file should be rejected");

    let err = result.unwrap_err();
    let err_msg = err.to_string().to_lowercase();

    assert!(
        err_msg.contains("too short")
            || err_msg.contains("unexpected eof")
            || err_msg.contains("invalid format"),
        "error should mention file too short or format issue, got: {}",
        err
    );
}

#[test]
fn test_okb_wrong_password_rejected() {
    let path = std::path::Path::new("tests/data").join("okb_basic.okb");
    if !path.exists() {
        eprintln!("Skipping: okb_basic.okb not found");
        return;
    }

    let wrong_pw = SecureStr::new("wrong-password".to_string());
    let parser = OkbParser;
    let result = parser.parse(&path, Some(&wrong_pw), None);

    assert!(result.is_err(), "wrong password should be rejected");

    let err = result.unwrap_err();
    let err_msg = err.to_string().to_lowercase();

    assert!(
        err_msg.contains("decryption") || err_msg.contains("password") || err_msg.contains("auth"),
        "error should mention decryption or password issue, got: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// Helper Functions for Full Pipeline Tests
// ---------------------------------------------------------------------------

/// Set up an in-memory vault for testing.
fn setup_vault() -> VaultService {
    let conn = init_db_in_memory();
    let mut svc = VaultService::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
    svc.unlock_with_mnemonic(&mnemonic).unwrap();
    svc
}

/// Convert field map to EncryptedPayload based on credential type.
fn fields_to_payload(
    cred_type: CredentialType,
    fields: &std::collections::HashMap<String, String>,
) -> EncryptedPayload {
    match cred_type {
        CredentialType::Login => EncryptedPayload::Login {
            name: fields.get("name").cloned().unwrap_or_default(),
            username: fields.get("username").cloned().unwrap_or_default(),
            password: SecureStr::new(fields.get("password").cloned().unwrap_or_default()),
            url: fields.get("url").cloned(),
            notes: fields.get("notes").cloned(),
        },
        CredentialType::Api => EncryptedPayload::Api {
            name: fields.get("name").cloned().unwrap_or_default(),
            app_id: fields.get("app_id").cloned().unwrap_or_default(),
            secret_key: SecureStr::new(fields.get("secret_key").cloned().unwrap_or_default()),
            url: fields.get("url").cloned(),
            notes: fields.get("notes").cloned(),
        },
        CredentialType::Ssh => EncryptedPayload::Ssh {
            name: fields.get("name").cloned().unwrap_or_default(),
            public_key: fields.get("public_key").cloned().unwrap_or_default(),
            private_key: fields.get("private_key").cloned().map(SecureStr::new),
            passphrase: fields.get("passphrase").cloned().map(SecureStr::new),
            notes: fields.get("notes").cloned(),
        },
    }
}

/// Get default sort configuration.
#[allow(dead_code)]
fn default_sort() -> RecordSort {
    RecordSort::default()
}

// ---------------------------------------------------------------------------
// Full Pipeline Integration Tests
// ---------------------------------------------------------------------------

#[test]
fn test_okb_basic_full_import() {
    let path = std::path::Path::new("tests/data").join("okb_basic.okb");
    if !path.exists() {
        return;
    }

    let mut vault = setup_vault();
    let mut svc = ImportExportService::new();

    let session_id = svc
        .create_import_session(
            ImportSource::OpenKeyringBackup,
            path,
            Some(SecureStr::new("test-password".to_string())),
            None,
            false,
        )
        .expect("create session");

    let preview = svc.validate_import_file(session_id).expect("validate");

    assert_eq!(preview.importable, 3, "all 3 items should pass validation");
    assert_eq!(preview.failed, 0, "no validation failures");

    let existing_keys: HashSet<ExistingRecordKey> = HashSet::new();
    let result = svc
        .execute_import(
            session_id,
            existing_keys,
            |cred_type, fields, tags| {
                let payload = fields_to_payload(cred_type, &fields);
                vault
                    .create_record(CreateRecordParams {
                        credential_type: cred_type,
                        payload,
                        tags,
                        is_favorite: false,
                        expires_at: None,
                    })
                    .map_err(|e| e.to_string())
            },
            None::<fn(usize, usize, &str)>,
        )
        .expect("execute import");

    assert_eq!(result.imported, 3, "should import 3 records");
    assert_eq!(result.failed, 0, "no failures");
    assert_eq!(result.validation_failed, 0, "no validation failures");

    // Verify records in vault
    let records = vault
        .list_records(&RecordFilter::All, &default_sort())
        .unwrap();
    assert_eq!(records.len(), 3, "vault should have 3 records");

    let names: Vec<&str> = records.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"GitHub"));
    assert!(names.contains(&"Gmail"));
    assert!(names.contains(&"AWS Console"));
}

#[test]
fn test_okb_mixed_types_full_import() {
    let path = std::path::Path::new("tests/data").join("okb_mixed_types.okb");
    if !path.exists() {
        return;
    }

    let mut vault = setup_vault();
    let mut svc = ImportExportService::new();

    let session_id = svc
        .create_import_session(
            ImportSource::OpenKeyringBackup,
            path,
            Some(SecureStr::new("test-password".to_string())),
            None,
            false,
        )
        .expect("create session");

    svc.validate_import_file(session_id).expect("validate");
    let existing_keys: HashSet<ExistingRecordKey> = HashSet::new();
    let result = svc
        .execute_import(
            session_id,
            existing_keys,
            |cred_type, fields, tags| {
                let payload = fields_to_payload(cred_type, &fields);
                vault
                    .create_record(CreateRecordParams {
                        credential_type: cred_type,
                        payload,
                        tags,
                        is_favorite: false,
                        expires_at: None,
                    })
                    .map_err(|e| e.to_string())
            },
            None::<fn(usize, usize, &str)>,
        )
        .expect("execute import");

    assert_eq!(result.imported, 3, "should import 3 records");
    assert_eq!(result.validation_failed, 0);

    let records = vault
        .list_records(&RecordFilter::All, &default_sort())
        .unwrap();
    assert_eq!(records.len(), 3);

    // Verify all credential types present
    let types: Vec<String> = records
        .iter()
        .map(|r| format!("{:?}", r.credential_type))
        .collect();
    assert!(types.iter().any(|t| t == "Login"), "should have Login");
    assert!(types.iter().any(|t| t == "Api"), "should have Api");
    assert!(types.iter().any(|t| t == "Ssh"), "should have Ssh");
}

#[test]
fn test_okb_edge_cases_full_import() {
    let path = std::path::Path::new("tests/data").join("okb_edge_cases.okb");
    if !path.exists() {
        return;
    }

    let mut vault = setup_vault();
    let mut svc = ImportExportService::new();

    let session_id = svc
        .create_import_session(
            ImportSource::OpenKeyringBackup,
            path,
            Some(SecureStr::new("test-password".to_string())),
            None,
            false,
        )
        .expect("create session");

    svc.validate_import_file(session_id).expect("validate");
    let existing_keys: HashSet<ExistingRecordKey> = HashSet::new();
    let result = svc
        .execute_import(
            session_id,
            existing_keys,
            |cred_type, fields, tags| {
                let payload = fields_to_payload(cred_type, &fields);
                vault
                    .create_record(CreateRecordParams {
                        credential_type: cred_type,
                        payload,
                        tags,
                        is_favorite: false,
                        expires_at: None,
                    })
                    .map_err(|e| e.to_string())
            },
            None::<fn(usize, usize, &str)>,
        )
        .expect("execute import");

    assert_eq!(result.imported, 4, "should import 4 records");
    assert_eq!(result.validation_failed, 0);

    let records = vault
        .list_records(&RecordFilter::All, &default_sort())
        .unwrap();
    assert_eq!(records.len(), 4);

    let names: Vec<String> = records.iter().map(|r| r.name.clone()).collect();
    assert!(
        names.iter().any(|n| n.contains(',')),
        "should have special chars name"
    );
    assert!(names.contains(&"Long Notes".to_string()));
    assert!(
        names.contains(&"测试账户".to_string()),
        "should have Chinese name"
    );
    assert!(names.contains(&"Minimal Entry".to_string()));
}
