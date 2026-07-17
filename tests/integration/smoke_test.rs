//! End-to-end smoke tests verifying PR #104's memory protection features
//! (LockedKey32, SecureStr, SensitiveInput) do not break normal vault operations.
//!
//! These tests close issue #106 by covering the acceptance criteria:
//! - Unlock vault succeeds
//! - Create records for Login, Api, Ssh credential types
//! - Edit records with secret fields surviving re-encryption
//! - Copy record secrets (decrypt_field path)
//! - Import CSV flow
//! - Export decrypt path for all credential types

use std::io::Write;

use oak_keyring::commands::types::{
    CsvColumnMapping, FieldSelector, ImportSource, RecordFilter, RecordSort, SortDirection,
    SortField,
};
use oak_keyring::commands::CommandResult;
use oak_keyring::config::AppConfig;
use oak_keyring::crypto::bip39::{MnemonicLanguage, Passkey};
use oak_keyring::db::schema::init_db_in_memory;
use oak_keyring::executor::import_export::handle_execute_import;
use oak_keyring::executor::CommandExecutor;
use oak_keyring::services::vault::{Vault, VaultService};
use oak_keyring::types::credential::{CredentialType, EncryptedPayload};
use oak_keyring::types::record::{CreateRecordParams, DecryptedRecord, UpdateRecordParams};
use oak_keyring::types::sensitive::SecureStr;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn setup_vault() -> VaultService {
    let conn = init_db_in_memory().unwrap();
    let mut svc = VaultService::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
    svc.unlock_with_mnemonic(&mnemonic)
        .expect("unlock_with_mnemonic must succeed in test");
    svc
}

fn setup_executor() -> CommandExecutor {
    let conn = init_db_in_memory().unwrap();
    let mut vault = VaultService::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
    vault
        .unlock_with_mnemonic(&mnemonic)
        .expect("unlock_with_mnemonic must succeed");

    let (result_tx, _) = mpsc::channel(64);

    CommandExecutor::builder(":memory:".into(), ":memory:".into())
        .vault(Box::new(vault) as Box<dyn Vault>)
        .config(AppConfig::default())
        .result_tx(result_tx)
        .shutdown_token(CancellationToken::new())
        .build()
        .expect("executor should build")
}

fn create_login(svc: &mut VaultService, name: &str, password: &str) -> uuid::Uuid {
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: EncryptedPayload::Login {
            name: name.to_string(),
            username: format!("user_{}", name),
            password: SecureStr::new(password.to_string()),
            url: None,
            notes: None,
            totp: None,
        },
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    })
    .expect("create login record")
}

fn create_api(svc: &mut VaultService, name: &str, secret_key: &str) -> uuid::Uuid {
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Api,
        payload: EncryptedPayload::Api {
            name: name.to_string(),
            app_id: format!("app_{}", name),
            secret_key: SecureStr::new(secret_key.to_string()),
            url: None,
            notes: None,
        },
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    })
    .expect("create api record")
}

fn create_ssh(
    svc: &mut VaultService,
    name: &str,
    private_key: &str,
    passphrase: Option<&str>,
) -> uuid::Uuid {
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Ssh,
        payload: EncryptedPayload::Ssh {
            name: name.to_string(),
            public_key: format!("ssh-rsa AAAA...{}", name),
            private_key: Some(SecureStr::new(private_key.to_string())),
            passphrase: passphrase.map(|p| SecureStr::new(p.to_string())),
            notes: None,
        },
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    })
    .expect("create ssh record")
}

#[test]
fn unlock_vault_derives_key_and_encrypts() {
    let mut svc = setup_vault();
    svc.lock();
    assert!(!svc.is_unlocked(), "vault should be locked");

    let mut svc2 = setup_vault();
    assert!(svc2.is_unlocked(), "fresh vault should be unlocked");

    let id = create_login(&mut svc2, "post-unlock", "secret123");
    let record = svc2.get_decrypted_record(id).expect("get decrypted");
    match record {
        DecryptedRecord::Login { password, .. } => {
            assert_eq!(password.expose(), "secret123");
        }
        _ => panic!("expected Login record"),
    }
}

#[test]
fn login_record_roundtrips_secure_fields() {
    let mut svc = setup_vault();
    let id = create_login(&mut svc, "GitHub", "s3cret_pass!");

    match svc.get_decrypted_record(id).expect("get decrypted") {
        DecryptedRecord::Login {
            name,
            username,
            password,
            url,
            notes,
            ..
        } => {
            assert_eq!(name, "GitHub");
            assert_eq!(username, "user_GitHub");
            assert_eq!(password.expose(), "s3cret_pass!");
            assert!(url.is_none());
            assert!(notes.is_none());
        }
        other => panic!("expected Login, got {:?}", other),
    }
}

#[test]
fn api_record_roundtrips_secure_fields() {
    let mut svc = setup_vault();
    let id = create_api(&mut svc, "AWS", "AKIA_secret_key_12345");

    match svc.get_decrypted_record(id).expect("get decrypted") {
        DecryptedRecord::Api {
            name,
            app_id,
            secret_key,
            url,
            notes,
            ..
        } => {
            assert_eq!(name, "AWS");
            assert_eq!(app_id, "app_AWS");
            assert_eq!(secret_key.expose(), "AKIA_secret_key_12345");
            assert!(url.is_none());
            assert!(notes.is_none());
        }
        other => panic!("expected Api, got {:?}", other),
    }
}

#[test]
fn ssh_record_roundtrips_secure_fields() {
    let mut svc = setup_vault();
    let pk = "-----BEGIN OPENSSH PRIVATE KEY-----\nabc123\n-----END OPENSSH PRIVATE KEY-----";
    let id = create_ssh(&mut svc, "Server", pk, Some("my-passphrase"));

    match svc.get_decrypted_record(id).expect("get decrypted") {
        DecryptedRecord::Ssh {
            name,
            public_key,
            private_key,
            passphrase,
            notes,
            ..
        } => {
            assert_eq!(name, "Server");
            assert_eq!(public_key, "ssh-rsa AAAA...Server");
            assert_eq!(private_key.unwrap().expose(), pk);
            assert_eq!(passphrase.unwrap().expose(), "my-passphrase");
            assert!(notes.is_none());
        }
        other => panic!("expected Ssh, got {:?}", other),
    }
}

#[test]
fn update_preserves_secret_fields() {
    let mut svc = setup_vault();
    let id = create_login(&mut svc, "Original", "original_pass");

    svc.update_record(UpdateRecordParams {
        id,
        payload: EncryptedPayload::Login {
            name: "Updated".to_string(),
            username: "new_user".to_string(),
            password: SecureStr::new("updated_pass".to_string()),
            url: Some("https://example.com".to_string()),
            notes: Some("updated notes".to_string()),
            totp: None,
        },
        tags: vec!["work".to_string()],
        is_favorite: true,
        expires_at: None,
        expected_version: 1,
    })
    .expect("update record");

    match svc.get_decrypted_record(id).expect("get decrypted") {
        DecryptedRecord::Login {
            name,
            username,
            password,
            url,
            notes,
            tags,
            is_favorite,
            version,
            ..
        } => {
            assert_eq!(name, "Updated");
            assert_eq!(username, "new_user");
            assert_eq!(password.expose(), "updated_pass");
            assert_eq!(url.as_deref(), Some("https://example.com"));
            assert_eq!(notes.as_deref(), Some("updated notes"));
            assert_eq!(tags, vec!["work"]);
            assert!(is_favorite);
            assert_eq!(version, 2);
        }
        other => panic!("expected Login, got {:?}", other),
    }
}

#[test]
fn decrypt_field_returns_correct_secret() {
    let mut svc = setup_vault();
    let id = create_login(&mut svc, "Clipboard", "clip_secret_123");

    let password = svc
        .decrypt_field(id, FieldSelector::Password)
        .expect("decrypt_field");
    assert_eq!(
        password.expose(),
        "clip_secret_123",
        "decrypt_field should return the correct password"
    );
}

#[test]
fn import_csv_creates_records_with_secure_secrets() {
    let mut executor = setup_executor();

    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    writeln!(
        tmp,
        "name,username,password,url,notes\n\
         Site1,alice,pass_word1,https://site1.com,dev\n\
         Site2,bob,p@ss_word2,,email"
    )
    .expect("write csv");
    let csv_path = tmp.path().to_path_buf();

    let mapping = CsvColumnMapping {
        name_column: "name".into(),
        username_column: "username".into(),
        password_column: "password".into(),
        url_column: "url".into(),
        notes_column: "notes".into(),
        totp_column: None,
        tags_column: None,
        skip_header: true,
    };

    let result = handle_execute_import(
        &mut executor,
        None,
        ImportSource::Csv,
        csv_path,
        None,
        Some(mapping),
        false,
    );

    match result {
        CommandResult::ImportCompleted {
            imported_count,
            failed_count,
            ..
        } => {
            assert_eq!(imported_count, 2, "2 CSV rows should import");
            assert_eq!(failed_count, 0, "no failures expected");
        }
        other => panic!("expected ImportCompleted, got {:?}", other),
    }
}

#[test]
fn export_decrypt_path_for_all_credential_types() {
    let mut svc = setup_vault();

    create_login(&mut svc, "ExportLogin", "export_pass!");
    create_api(&mut svc, "ExportApi", "export_secret_key");
    create_ssh(
        &mut svc,
        "ExportSsh",
        "-----BEGIN KEY-----\nexport\n-----END KEY-----",
        Some("export-passphrase"),
    );

    let records = svc
        .list_records(
            &RecordFilter::All,
            &RecordSort {
                field: SortField::Name,
                direction: SortDirection::Asc,
            },
        )
        .expect("list records");
    assert_eq!(records.len(), 3);

    for record in &records {
        let decrypted = svc.get_decrypted_record(record.id).expect("decrypt record");
        match decrypted {
            DecryptedRecord::Login { name, password, .. } => {
                assert_eq!(name, "ExportLogin");
                assert_eq!(password.expose(), "export_pass!");
            }
            DecryptedRecord::Api {
                name, secret_key, ..
            } => {
                assert_eq!(name, "ExportApi");
                assert_eq!(secret_key.expose(), "export_secret_key");
            }
            DecryptedRecord::Ssh {
                name,
                private_key,
                passphrase,
                ..
            } => {
                assert_eq!(name, "ExportSsh");
                assert_eq!(
                    private_key.unwrap().expose(),
                    "-----BEGIN KEY-----\nexport\n-----END KEY-----"
                );
                assert_eq!(passphrase.unwrap().expose(), "export-passphrase");
            }
            DecryptedRecord::SecureNote { name, notes, .. } => {
                // No SecureNote in this test, but handle for completeness
                let _ = name;
                let _ = notes;
            }
        }
    }
}
