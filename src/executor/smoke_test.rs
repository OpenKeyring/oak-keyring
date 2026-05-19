use std::io::Write;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::commands::types::{
    CsvColumnMapping, FieldSelector, ImportSource, RecordFilter, RecordSort, SortDirection,
    SortField,
};
use crate::commands::CommandResult;
use crate::config::AppConfig;
use crate::crypto::bip39::{MnemonicLanguage, Passkey};
use crate::executor::CommandExecutor;
use crate::services::clipboard::{ClipboardService, MockBackend};
use crate::services::vault::VaultServiceImpl;
use crate::types::record::{CreateRecordParams, DecryptedRecord, UpdateRecordParams};
use crate::types::{CredentialType, EncryptedPayload, SecureStr};

fn make_unlocked_executor() -> CommandExecutor {
    let conn = crate::db::schema::init_db_in_memory().unwrap();
    let mut vault = VaultServiceImpl::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
    vault
        .unlock_with_mnemonic(&mnemonic)
        .expect("unlock with mnemonic");

    let (result_tx, _) = mpsc::channel(64);

    CommandExecutor::builder(":memory:".into(), ":memory:".into())
        .vault(Box::new(vault))
        .config(AppConfig::default())
        .result_tx(result_tx)
        .shutdown_token(CancellationToken::new())
        .clipboard(Arc::new(ClipboardService::with_backend(
            Box::new(MockBackend::new()),
            30,
        )))
        .build()
}

fn create_login_record(executor: &mut CommandExecutor, name: &str, password: &str) -> uuid::Uuid {
    executor
        .vault_mut()
        .unwrap()
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: name.to_string(),
                username: format!("user_{}", name),
                password: SecureStr::new(password.to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create login record")
}

fn create_api_record(executor: &mut CommandExecutor, name: &str, secret_key: &str) -> uuid::Uuid {
    executor
        .vault_mut()
        .unwrap()
        .create_record(CreateRecordParams {
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

fn create_ssh_record(
    executor: &mut CommandExecutor,
    name: &str,
    private_key: &str,
    passphrase: Option<&str>,
) -> uuid::Uuid {
    executor
        .vault_mut()
        .unwrap()
        .create_record(CreateRecordParams {
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

// --- Test 1: Unlock vault with LockedKey32 derivation ---

#[test]
fn unlock_vault_with_password_derives_locked_key() {
    let mut executor = make_unlocked_executor();
    executor.vault_mut().unwrap().lock();
    assert!(
        !executor.is_unlocked(),
        "vault should be locked after lock()"
    );

    let conn = crate::db::schema::init_db_in_memory().unwrap();
    let mut vault2 = VaultServiceImpl::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
    vault2
        .unlock_with_mnemonic(&mnemonic)
        .expect("unlock with mnemonic");
    assert!(vault2.is_unlocked(), "vault should be unlocked");

    let id = vault2
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "post-unlock-test".to_string(),
                username: "user".to_string(),
                password: SecureStr::new("secret123".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create record after re-unlock");

    let record = vault2.get_decrypted_record(id).expect("get decrypted");
    match record {
        DecryptedRecord::Login { password, .. } => {
            assert_eq!(
                password.expose(),
                "secret123",
                "password should survive encrypt/decrypt"
            );
        }
        _ => panic!("expected Login record"),
    }
}

// --- Test 2: Login record SecureStr roundtrip ---

#[test]
fn create_login_record_roundtrips_secure_fields() {
    let mut executor = make_unlocked_executor();
    let id = create_login_record(&mut executor, "GitHub", "s3cret_pass!");

    let record = executor
        .vault_mut()
        .unwrap()
        .get_decrypted_record(id)
        .expect("get decrypted login");

    match record {
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
            assert_eq!(
                password.expose(),
                "s3cret_pass!",
                "SecureStr password survived encrypt/decrypt"
            );
            assert!(url.is_none());
            assert!(notes.is_none());
        }
        _ => panic!("expected Login record, got {:?}", record),
    }
}

// --- Test 3: Api record SecureStr roundtrip ---

#[test]
fn create_api_record_roundtrips_secure_fields() {
    let mut executor = make_unlocked_executor();
    let id = create_api_record(&mut executor, "AWS", "AKIA_secret_key_12345");

    let record = executor
        .vault_mut()
        .unwrap()
        .get_decrypted_record(id)
        .expect("get decrypted api");

    match record {
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
            assert_eq!(
                secret_key.expose(),
                "AKIA_secret_key_12345",
                "SecureStr secret_key survived encrypt/decrypt"
            );
            assert!(url.is_none());
            assert!(notes.is_none());
        }
        _ => panic!("expected Api record, got {:?}", record),
    }
}

// --- Test 4: Ssh record SecureStr roundtrip ---

#[test]
fn create_ssh_record_roundtrips_secure_fields() {
    let mut executor = make_unlocked_executor();
    let private_key =
        "-----BEGIN OPENSSH PRIVATE KEY-----\nabc123\n-----END OPENSSH PRIVATE KEY-----";
    let id = create_ssh_record(&mut executor, "Server", private_key, Some("my-passphrase"));

    let record = executor
        .vault_mut()
        .unwrap()
        .get_decrypted_record(id)
        .expect("get decrypted ssh");

    match record {
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
            assert!(private_key.is_some(), "private_key should be present");
            assert_eq!(
                private_key.unwrap().expose(),
                "-----BEGIN OPENSSH PRIVATE KEY-----\nabc123\n-----END OPENSSH PRIVATE KEY-----",
                "SecureStr private_key survived encrypt/decrypt"
            );
            assert!(passphrase.is_some(), "passphrase should be present");
            assert_eq!(
                passphrase.unwrap().expose(),
                "my-passphrase",
                "SecureStr passphrase survived encrypt/decrypt"
            );
            assert!(notes.is_none());
        }
        _ => panic!("expected Ssh record, got {:?}", record),
    }
}

// --- Test 5: Update preserves secret fields through re-encryption ---

#[test]
fn update_record_preserves_secret_fields() {
    let mut executor = make_unlocked_executor();
    let id = create_login_record(&mut executor, "Original", "original_pass");

    executor
        .vault_mut()
        .unwrap()
        .update_record(UpdateRecordParams {
            id,
            payload: EncryptedPayload::Login {
                name: "Updated".to_string(),
                username: "new_user".to_string(),
                password: SecureStr::new("updated_pass".to_string()),
                url: Some("https://example.com".to_string()),
                notes: Some("updated notes".to_string()),
            },
            tags: vec!["work".to_string()],
            is_favorite: true,
            expires_at: None,
            expected_version: 1,
        })
        .expect("update record");

    let record = executor
        .vault_mut()
        .unwrap()
        .get_decrypted_record(id)
        .expect("get decrypted after update");

    match record {
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
            assert_eq!(name, "Updated", "name should be updated");
            assert_eq!(username, "new_user");
            assert_eq!(
                password.expose(),
                "updated_pass",
                "SecureStr password survived update re-encryption"
            );
            assert_eq!(url.as_deref(), Some("https://example.com"));
            assert_eq!(notes.as_deref(), Some("updated notes"));
            assert_eq!(tags, vec!["work"]);
            assert!(is_favorite);
            assert_eq!(version, 2, "version should be incremented after update");
        }
        _ => panic!("expected Login record"),
    }
}

// --- Test 6: Clipboard copy with SecureStr ---

#[tokio::test]
async fn copy_password_to_clipboard_works() {
    let mut executor = make_unlocked_executor();
    let id = create_login_record(&mut executor, "Clipboard", "clip_secret_123");

    let result = crate::executor::clipboard::handle_copy_to_clipboard(
        &mut executor,
        id,
        FieldSelector::Password,
    )
    .await;

    match result {
        CommandResult::CopiedToClipboard {
            field,
            clear_after_seconds,
        } => {
            assert_eq!(field, FieldSelector::Password);
            assert!(clear_after_seconds > 0, "clear timer should be set");
        }
        other => panic!("expected CopiedToClipboard, got {:?}", other),
    }
}

// --- Test 7: Import CSV creates records with SecureStr secrets ---

#[test]
fn import_csv_creates_records_with_secure_secrets() {
    let mut executor = make_unlocked_executor();

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
        tags_column: None,
        skip_header: true,
    };

    let result = crate::executor::import_export::handle_execute_import(
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

    let records = executor
        .vault_mut()
        .unwrap()
        .list_records(
            &RecordFilter::All,
            &RecordSort {
                field: SortField::Name,
                direction: SortDirection::Asc,
            },
        )
        .expect("list records");
    assert_eq!(records.len(), 2, "vault should have 2 imported records");

    let site1_id = records
        .iter()
        .find(|r| r.name == "Site1")
        .expect("find Site1")
        .id;
    let decrypted = executor
        .vault_mut()
        .unwrap()
        .get_decrypted_record(site1_id)
        .expect("decrypt Site1");

    match decrypted {
        DecryptedRecord::Login {
            password, username, ..
        } => {
            assert_eq!(username, "alice");
            assert_eq!(
                password.expose(),
                "pass_word1",
                "imported password survived encrypt/decrypt"
            );
        }
        _ => panic!("expected Login record"),
    }
}

// --- Test 8: Export verifies decrypt path for all credential types ---

#[test]
fn export_verifies_decrypt_path_for_all_credential_types() {
    let mut executor = make_unlocked_executor();

    create_login_record(&mut executor, "ExportLogin", "export_pass!");
    create_api_record(&mut executor, "ExportApi", "export_secret_key");
    create_ssh_record(
        &mut executor,
        "ExportSsh",
        "-----BEGIN KEY-----\nexport\n-----END KEY-----",
        Some("export-passphrase"),
    );

    let records = executor
        .vault_mut()
        .unwrap()
        .list_records(
            &RecordFilter::All,
            &RecordSort {
                field: SortField::Name,
                direction: SortDirection::Asc,
            },
        )
        .expect("list records");
    assert_eq!(records.len(), 3, "should have 3 records");

    for record in &records {
        let decrypted = executor
            .vault_mut()
            .unwrap()
            .get_decrypted_record(record.id)
            .expect("decrypt record");
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
        }
    }
}
