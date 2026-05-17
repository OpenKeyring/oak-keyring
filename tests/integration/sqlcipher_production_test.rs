use oak_keyring::crypto::argon2::Argon2Params;
use oak_keyring::crypto::bip39::MnemonicLanguage;
use oak_keyring::crypto::db_page_key::test_db_page_key;
use oak_keyring::crypto::keystore::KeyStore;
use oak_keyring::db::vault_db::{VaultDbError, VaultDbFactory};
use oak_keyring::executor::{CommandExecutor, DbStartupMode};
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
    )
    .expect("executor starts locked");

    let result = oak_keyring::executor::vault::handle_unlock(&mut executor, password).await;
    assert!(matches!(
        result,
        oak_keyring::commands::CommandResult::VaultUnlocked
    ));
    assert!(executor.is_unlocked());
}
