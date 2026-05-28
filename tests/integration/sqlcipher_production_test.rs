use oak_keyring::commands::types::{RecordFilter, RecordSort, SortDirection, SortField};
use oak_keyring::commands::CommandResult;
use oak_keyring::crypto::argon2::Argon2Params;
use oak_keyring::crypto::bip39::MnemonicLanguage;
use oak_keyring::crypto::db_page_key::test_db_page_key;
use oak_keyring::crypto::keystore::KeyStore;
use oak_keyring::db::vault_db::{VaultDbError, VaultDbFactory};
use oak_keyring::executor::{ActivityTracker, CommandExecutor, DbStartupMode};
use oak_keyring::types::credential::{CredentialType, EncryptedPayload};
use oak_keyring::types::SecureStr;
use rusqlite::Connection;
use tempfile::TempDir;

#[test]
fn factory_creates_sqlcipher_vault_unreadable_without_key() {
    let dir = TempDir::new().expect("temp dir");
    let key = test_db_page_key([0x11; 32]);

    let conn = VaultDbFactory::create_sqlcipher_vault(dir.path(), &key).expect("create vault");
    conn.execute("INSERT INTO tags (name) VALUES ('factory-secret')", [])
        .expect("insert tag");
    drop(conn);

    let plain = Connection::open(dir.path().join("vault.db")).expect("plain handle");
    let result: rusqlite::Result<i64> =
        plain.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0));
    assert!(
        result.is_err(),
        "plain SQLite must not read SQLCipher schema"
    );
}

#[tokio::test]
async fn new_vault_creates_sqlcipher_database_directly() {
    let dir = TempDir::new().expect("temp dir");
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut executor = CommandExecutor::new(
        oak_keyring::config::AppConfig::default_config(),
        tx,
        tokio_util::sync::CancellationToken::new(),
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        DbStartupMode::DeferredInMemory,
        ActivityTracker::new(),
    )
    .expect("executor");

    let result = oak_keyring::executor::vault::handle_initialize_vault(
        &mut executor,
        SecureStr::new("new vault password".to_string()),
        None,
    )
    .await;
    assert!(matches!(result, CommandResult::VaultInitialized));

    let plain = Connection::open(dir.path().join("vault.db")).expect("plain handle");
    let read_schema: rusqlite::Result<i64> =
        plain.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0));
    assert!(
        read_schema.is_err(),
        "new vault db must be SQLCipher encrypted"
    );
}

#[test]
fn factory_rejects_plaintext_database() {
    let dir = TempDir::new().expect("temp dir");
    Connection::open(dir.path().join("vault.db"))
        .expect("plain db")
        .execute("CREATE TABLE plaintext_marker(id INTEGER PRIMARY KEY)", [])
        .expect("create plaintext table");

    let key = test_db_page_key([0x22; 32]);
    let err = VaultDbFactory::open_sqlcipher_vault(dir.path(), &key).unwrap_err();
    assert!(matches!(err, VaultDbError::PlaintextDatabaseUnsupported));
}

#[test]
fn factory_roundtrip_create_then_open() {
    let dir = TempDir::new().expect("temp dir");
    let key = test_db_page_key([0x33; 32]);

    let conn = VaultDbFactory::create_sqlcipher_vault(dir.path(), &key).expect("create vault");
    conn.execute("INSERT INTO tags (name) VALUES ('roundtrip')", [])
        .expect("insert tag");
    drop(conn);

    let conn2 = VaultDbFactory::open_sqlcipher_vault(dir.path(), &key).expect("open vault");
    let count: i64 = conn2
        .query_row(
            "SELECT COUNT(*) FROM tags WHERE name = 'roundtrip'",
            [],
            |row| row.get(0),
        )
        .expect("query tag");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn unlock_opens_sqlcipher_only_after_keystore_unlock() {
    let dir = TempDir::new().expect("temp dir");
    let mut sk = [0x33u8; 32];
    let password = SecureStr::new("correct horse battery staple".to_string());
    KeyStore::initialize(
        dir.path(),
        &mut sk,
        &password,
        &Argon2Params::medium(),
        MnemonicLanguage::English,
    )
    .expect("initialize key file");

    let key = KeyStore::unlock(dir.path(), &password)
        .expect("unlock keystore")
        .db_page_key()
        .expect("derive db key");
    VaultDbFactory::create_sqlcipher_vault(dir.path(), &key).expect("create encrypted db");

    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut executor = CommandExecutor::new(
        oak_keyring::config::AppConfig::default_config(),
        tx,
        tokio_util::sync::CancellationToken::new(),
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        DbStartupMode::DeferredInMemory,
        ActivityTracker::new(),
    )
    .expect("executor starts locked");

    let result = oak_keyring::executor::vault::handle_unlock(&mut executor, password).await;
    assert!(matches!(
        result,
        oak_keyring::commands::CommandResult::VaultUnlocked
    ));
    assert!(executor.is_unlocked());
}

#[test]
fn pending_sqlcipher_guard_rolls_back_uncommitted_database_files() {
    let dir = TempDir::new().expect("temp dir");
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut executor = CommandExecutor::new(
        oak_keyring::config::AppConfig::default_config(),
        tx,
        tokio_util::sync::CancellationToken::new(),
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        DbStartupMode::DeferredInMemory,
        ActivityTracker::new(),
    )
    .expect("executor");

    let key = test_db_page_key([0x44; 32]);
    // Start a pending file-backed guard by calling begin_file_backed_vault_db.
    // The guard rolls back newly created database files on drop when not committed.
    {
        let _guard = executor
            .begin_file_backed_vault_db(&key)
            .expect("begin pending");
        // The SQLCipher vault.db file should exist after begin
        assert!(
            dir.path().join("vault.db").exists(),
            "vault.db should exist after begin_file_backed_vault_db"
        );
        // Don't commit — guard should roll back uncommitted files on drop
    }
    // After the guard is dropped without commit, all files created by
    // begin_file_backed_vault_db should be removed.
    assert!(
        !dir.path().join("vault.db").exists(),
        "vault.db should be rolled back after guard drop without commit"
    );
    // WAL and SHM files may or may not have been created; if they were, they
    // must also be removed by the rollback.
    assert!(
        !dir.path().join("vault.db-wal").exists(),
        "vault.db-wal should be rolled back after guard drop without commit"
    );
}

#[tokio::test]
async fn lock_drops_open_sqlcipher_runtime() {
    let dir = TempDir::new().expect("temp dir");
    // Set up an unlocked executor with SQLCipher vault
    let mut sk = [0x55u8; 32];
    let password = SecureStr::new("lock test password".to_string());
    KeyStore::initialize(
        dir.path(),
        &mut sk,
        &password,
        &Argon2Params::medium(),
        MnemonicLanguage::English,
    )
    .expect("initialize key file");

    let key = KeyStore::unlock(dir.path(), &password)
        .expect("unlock keystore")
        .db_page_key()
        .expect("derive db key");
    VaultDbFactory::create_sqlcipher_vault(dir.path(), &key).expect("create encrypted db");

    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut executor = CommandExecutor::new(
        oak_keyring::config::AppConfig::default_config(),
        tx,
        tokio_util::sync::CancellationToken::new(),
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        DbStartupMode::DeferredInMemory,
        ActivityTracker::new(),
    )
    .expect("executor starts locked");

    // Unlock first (password is moved here — all borrows above released)
    let unlock_result = oak_keyring::executor::vault::handle_unlock(&mut executor, password).await;
    assert!(matches!(unlock_result, CommandResult::VaultUnlocked));
    assert!(executor.is_unlocked());

    // Lock
    let result = oak_keyring::executor::vault::handle_lock(&mut executor);
    assert!(matches!(result, CommandResult::VaultLocked));
    assert!(!executor.is_unlocked());
}

#[tokio::test]
async fn sqlcipher_wal_does_not_contain_plaintext_secrets() {
    let dir = TempDir::new().expect("temp dir");
    let mut sk = [0x66u8; 32];
    let password = SecureStr::new("wal test password".to_string());
    KeyStore::initialize(
        dir.path(),
        &mut sk,
        &password,
        &Argon2Params::medium(),
        MnemonicLanguage::English,
    )
    .expect("initialize key file");

    let key = KeyStore::unlock(dir.path(), &password)
        .expect("unlock keystore")
        .db_page_key()
        .expect("derive db key");
    let conn =
        VaultDbFactory::create_sqlcipher_vault(dir.path(), &key).expect("create encrypted db");

    // Write a marker and checkpoint
    conn.execute("INSERT INTO tags (name) VALUES ('wal-marker')", [])
        .expect("insert marker");
    oak_keyring::db::schema::checkpoint_wal(&conn).expect("checkpoint wal");
    drop(conn);

    // Scan main DB file for the marker - should NOT appear in plaintext
    let db_bytes = std::fs::read(dir.path().join("vault.db")).expect("read vault.db");
    // The marker "wal-marker" should not be readable in the encrypted DB
    assert!(
        !contains_plaintext(&db_bytes, b"wal-marker"),
        "vault.db must not contain plaintext marker"
    );
}

#[tokio::test]
async fn sqlcipher_unlocked_vault_supports_full_query_pipeline() {
    let dir = TempDir::new().expect("temp dir");
    let mut sk = [0x77u8; 32];
    KeyStore::initialize(
        dir.path(),
        &mut sk,
        &SecureStr::new("query pipeline password".to_string()),
        &Argon2Params::medium(),
        MnemonicLanguage::English,
    )
    .expect("initialize");
    let key = KeyStore::unlock(
        dir.path(),
        &SecureStr::new("query pipeline password".to_string()),
    )
    .expect("unlock ks")
    .db_page_key()
    .expect("key");
    VaultDbFactory::create_sqlcipher_vault(dir.path(), &key).expect("create db");

    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut executor = CommandExecutor::new(
        oak_keyring::config::AppConfig::default_config(),
        tx,
        tokio_util::sync::CancellationToken::new(),
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        DbStartupMode::DeferredInMemory,
        ActivityTracker::new(),
    )
    .expect("executor");

    let unlock_result = oak_keyring::executor::vault::handle_unlock(
        &mut executor,
        SecureStr::new("query pipeline password".to_string()),
    )
    .await;
    assert!(matches!(unlock_result, CommandResult::VaultUnlocked));

    // Create a record through the executor's public record handler
    let create_result = oak_keyring::executor::record::handle_create_record(
        &mut executor,
        CredentialType::Login,
        EncryptedPayload::Login {
            name: "test-login".to_string(),
            username: "user@test.com".to_string(),
            password: SecureStr::new("test-password".to_string()),
            url: Some("https://test.example.com".to_string()),
            notes: Some("regression test".to_string()),
        },
        vec!["test-tag".to_string()],
        false,
        None,
    );
    let created_id = match &create_result {
        CommandResult::RecordCreated { id } => *id,
        other => panic!("Expected RecordCreated, got {:?}", other),
    };

    // List records via public handler
    let list_result = oak_keyring::executor::record::handle_load_record_list(
        &mut executor,
        RecordFilter::All,
        RecordSort {
            field: SortField::Name,
            direction: SortDirection::Asc,
        },
    );
    let records = match &list_result {
        CommandResult::RecordListLoaded { records, .. } => records,
        other => panic!("Expected RecordListLoaded, got {:?}", other),
    };
    assert!(!records.is_empty(), "must have at least one record");

    // Search
    let search_result = oak_keyring::executor::record::handle_load_record_list(
        &mut executor,
        RecordFilter::Search("test-login".to_string()),
        RecordSort {
            field: SortField::Name,
            direction: SortDirection::Asc,
        },
    );
    let search_records = match &search_result {
        CommandResult::RecordListLoaded { records, .. } => records,
        other => panic!("Expected RecordListLoaded, got {:?}", other),
    };
    assert_eq!(search_records.len(), 1);

    // Check the created record is the one we found
    assert_eq!(search_records[0].id, created_id);

    // Tag list via public handler
    let tags_result = oak_keyring::executor::record::handle_load_tags(&mut executor);
    let tags = match &tags_result {
        CommandResult::TagsLoaded { tags, .. } => tags,
        other => panic!("Expected TagsLoaded, got {:?}", other),
    };
    assert!(
        tags.iter().any(|tag| tag.name == "test-tag"),
        "must find test-tag"
    );

    // Audit log: create_record writes audit entries, load all entries
    let audit_result = oak_keyring::executor::config::handle_load_audit_log(
        &mut executor,
        oak_keyring::commands::types::AuditFilter {
            operation: None,
            time_range: None,
            search: None,
        },
    );
    match audit_result {
        CommandResult::AuditLogLoaded { entries, .. } => {
            assert!(!entries.is_empty(), "audit must have create entry");
        }
        other => panic!("Expected AuditLogLoaded, got {:?}", other),
    }

    // Metadata is exercised implicitly: unlock persisted device_id, create
    // record wrote metadata_version, audit log built from metadata entries.

    // Health state: schedule a health check against the encrypted vault.
    // Health checks run asynchronously; force=true returns HealthCheckStarted.
    let health_result = oak_keyring::executor::health::handle_run_health_check(&mut executor, true);
    assert!(
        matches!(
            &health_result,
            CommandResult::HealthCheckStarted | CommandResult::HealthCheckCompleted { .. }
        ),
        "Expected HealthCheckStarted or HealthCheckCompleted, got {:?}",
        health_result
    );

    // Export: export vault to an .okb file (exercises decrypt+serialize path)
    let export_path = dir.path().join("export.okb");
    let master_password = SecureStr::new("query pipeline password".to_string());
    let export_result = oak_keyring::executor::import_export::handle_execute_export(
        &mut executor,
        oak_keyring::commands::types::ExportScope::All,
        export_path.clone(),
        SecureStr::new("export-password".to_string()),
        master_password,
        oak_keyring::commands::types::ExportFormat::Okb,
    );
    match &export_result {
        CommandResult::ExportCompleted { .. } => {}
        other => panic!("Expected ExportCompleted, got {:?}", other),
    }
    let export_meta = std::fs::metadata(&export_path).expect("okb file should exist");
    assert!(export_meta.len() > 0, "okb file must be non-empty");

    // Verify plain SQLite cannot read
    let plain = Connection::open(dir.path().join("vault.db")).expect("plain handle");
    let read_schema: rusqlite::Result<i64> =
        plain.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0));
    assert!(
        read_schema.is_err(),
        "plain SQLite must not read SQLCipher schema"
    );
}

#[tokio::test]
async fn change_master_password_preserves_sqlcipher_db_access() {
    let dir = TempDir::new().expect("temp dir");
    let mut sk = [0x88u8; 32];
    let old_password_text = "old master password";
    let new_password_text = "new master password";

    KeyStore::initialize(
        dir.path(),
        &mut sk,
        &SecureStr::new(old_password_text.to_string()),
        &Argon2Params::medium(),
        MnemonicLanguage::English,
    )
    .expect("initialize");
    let key = KeyStore::unlock(dir.path(), &SecureStr::new(old_password_text.to_string()))
        .expect("unlock ks")
        .db_page_key()
        .expect("key");
    VaultDbFactory::create_sqlcipher_vault(dir.path(), &key).expect("create db");

    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut executor = CommandExecutor::new(
        oak_keyring::config::AppConfig::default_config(),
        tx,
        tokio_util::sync::CancellationToken::new(),
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        DbStartupMode::DeferredInMemory,
        ActivityTracker::new(),
    )
    .expect("executor");

    // Unlock with old password
    let unlock = oak_keyring::executor::vault::handle_unlock(
        &mut executor,
        SecureStr::new(old_password_text.to_string()),
    )
    .await;
    assert!(matches!(unlock, CommandResult::VaultUnlocked));

    // Change master password
    let result = oak_keyring::executor::vault::handle_change_master_password(
        &mut executor,
        Some(SecureStr::new(old_password_text.to_string())),
        SecureStr::new(new_password_text.to_string()),
    );
    assert!(matches!(result, CommandResult::MasterPasswordChanged));

    // Lock and unlock with new password
    oak_keyring::executor::vault::handle_lock(&mut executor);

    let unlock_new = oak_keyring::executor::vault::handle_unlock(
        &mut executor,
        SecureStr::new(new_password_text.to_string()),
    )
    .await;
    assert!(matches!(unlock_new, CommandResult::VaultUnlocked));
    assert!(executor.is_unlocked());
}

#[test]
fn plaintext_database_error_is_distinguishable() {
    let dir = TempDir::new().expect("temp dir");
    // Create a plaintext SQLite database
    Connection::open(dir.path().join("vault.db"))
        .expect("plain db")
        .execute("CREATE TABLE plaintext_marker(id INTEGER PRIMARY KEY)", [])
        .expect("create table");

    let key = test_db_page_key([0x99; 32]);
    let err = VaultDbFactory::open_sqlcipher_vault(dir.path(), &key).unwrap_err();
    assert!(
        matches!(err, VaultDbError::PlaintextDatabaseUnsupported),
        "plaintext should produce PlaintextDatabaseUnsupported, got {:?}",
        err
    );
}

#[test]
fn wrong_key_error_is_distinguishable() {
    let dir = TempDir::new().expect("temp dir");
    // Create with one key
    let key1 = test_db_page_key([0xaa; 32]);
    VaultDbFactory::create_sqlcipher_vault(dir.path(), &key1).expect("create with key1");

    // Try to open with different key.
    // The cipher_version probe in open_keyed_connection validates the key
    // material on first read through SQLCipher's encryption layer.
    let key2 = test_db_page_key([0xbb; 32]);
    let err = VaultDbFactory::open_sqlcipher_vault(dir.path(), &key2).unwrap_err();
    assert!(
        matches!(err, VaultDbError::WrongDbPageKey),
        "wrong key should produce WrongDbPageKey, got {:?}",
        err
    );
}

#[test]
fn unsupported_schema_version_error_is_distinguishable() {
    let dir = TempDir::new().expect("temp dir");
    let key = test_db_page_key([0xcc; 32]);
    let conn = VaultDbFactory::create_sqlcipher_vault(dir.path(), &key).expect("create db");

    // Write an impossibly high schema version into the metadata table
    conn.execute(
        "UPDATE metadata SET value = '999999' WHERE key = 'schema_version'",
        [],
    )
    .expect("set impossible version");
    drop(conn);

    // Try to open - should get UnsupportedSchemaVersion
    let err = VaultDbFactory::open_sqlcipher_vault(dir.path(), &key).unwrap_err();
    assert!(
        matches!(err, VaultDbError::UnsupportedSchemaVersion { .. }),
        "unsupported version should produce UnsupportedSchemaVersion, got {:?}",
        err
    );
}

fn contains_plaintext(data: &[u8], pattern: &[u8]) -> bool {
    data.windows(pattern.len()).any(|window| window == pattern)
}
