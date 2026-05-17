use oak_keyring::crypto::db_page_key::test_db_page_key;
use oak_keyring::db::vault_db::{VaultDbError, VaultDbFactory};
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
