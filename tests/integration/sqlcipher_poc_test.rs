use std::path::Path;

use oak_keyring::db::migrations::MigrationError;
use oak_keyring::db::schema::run_with_encrypted_backup;
use oak_keyring::db::sqlcipher::{
    apply_key, cipher_version, open_encrypted_connection, open_encrypted_vault_dir,
};
use rusqlite::Connection;
use tempfile::TempDir;

fn test_key() -> [u8; 32] {
    [7u8; 32]
}

fn assert_plain_sqlite_schema_read_fails(db_path: &Path) {
    let plain = Connection::open(db_path).expect("plain sqlite open handle");
    let result: rusqlite::Result<i64> =
        plain.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0));
    assert!(
        result.is_err(),
        "plain SQLite connection without SQLCipher key must not read schema"
    );
}

fn assert_file_does_not_contain(path: &Path, needle: &[u8]) {
    if !path.exists() {
        return;
    }

    let bytes = std::fs::read(path).expect("read database sidecar");
    assert!(
        !bytes.windows(needle.len()).any(|window| window == needle),
        "{} must not contain plaintext {:?}",
        path.display(),
        String::from_utf8_lossy(needle)
    );
}

#[test]
fn cipher_version_is_available() {
    let dir = TempDir::new().expect("temp dir");
    let conn = open_encrypted_vault_dir(dir.path(), &test_key()).expect("open encrypted db");
    let version = cipher_version(&conn).expect("cipher version");
    assert!(
        !version.trim().is_empty(),
        "SQLCipher must report a non-empty cipher_version"
    );
}

#[test]
fn encrypted_database_runs_migrations() {
    let dir = TempDir::new().expect("temp dir");
    let conn = open_encrypted_vault_dir(dir.path(), &test_key()).expect("open encrypted db");

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'records'",
            [],
            |row| row.get(0),
        )
        .expect("query sqlite_master");

    assert_eq!(count, 1, "records table must exist after migration");
}

#[test]
fn encrypted_database_reopens_with_same_key() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("vault.db");

    {
        let conn = open_encrypted_connection(&db_path, &test_key()).expect("open encrypted db");
        conn.execute("INSERT INTO tags (name) VALUES ('work')", [])
            .expect("insert tag");
    }

    let conn = open_encrypted_connection(&db_path, &test_key()).expect("reopen encrypted db");
    let tag: String = conn
        .query_row("SELECT name FROM tags", [], |row| row.get(0))
        .expect("read tag");
    assert_eq!(tag, "work");
}

#[test]
fn encrypted_database_rejects_plain_sqlite_open() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("vault.db");

    {
        let conn = open_encrypted_connection(&db_path, &test_key()).expect("open encrypted db");
        conn.execute("INSERT INTO tags (name) VALUES ('secret-tag')", [])
            .expect("insert tag");
    }

    assert_plain_sqlite_schema_read_fails(&db_path);
}

#[test]
fn encrypted_wal_and_shm_do_not_expose_plaintext() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("vault.db");
    let wal_path = dir.path().join("vault.db-wal");
    let shm_path = dir.path().join("vault.db-shm");

    let conn = open_encrypted_connection(&db_path, &test_key()).expect("open encrypted db");
    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("query journal mode");
    assert_eq!(
        journal_mode.to_ascii_lowercase(),
        "wal",
        "SQLCipher test database must be in WAL mode"
    );

    conn.execute("INSERT INTO tags (name) VALUES ('secret-wal-tag')", [])
        .expect("insert tag");

    assert_plain_sqlite_schema_read_fails(&db_path);
    assert!(wal_path.exists(), "WAL sidecar must exist for this check");
    assert!(shm_path.exists(), "SHM sidecar must exist for this check");
    assert_file_does_not_contain(&db_path, b"secret-wal-tag");
    assert_file_does_not_contain(&wal_path, b"secret-wal-tag");
    assert_file_does_not_contain(&shm_path, b"secret-wal-tag");
}

#[test]
fn encrypted_database_rejects_wrong_key() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("vault.db");
    let wrong_key = [9u8; 32];

    {
        let conn = open_encrypted_connection(&db_path, &test_key()).expect("open encrypted db");
        conn.execute("INSERT INTO tags (name) VALUES ('secret-tag')", [])
            .expect("insert tag");
    }

    let conn = Connection::open(&db_path).expect("open db handle");
    let read_result: rusqlite::Result<i64> = apply_key(&conn, &wrong_key)
        .and_then(|()| conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0)));

    assert!(
        read_result.is_err(),
        "wrong SQLCipher key must not read encrypted schema"
    );
}

#[test]
fn failed_migration_backup_path_does_not_create_plaintext_sqlite_backup() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("vault.db");
    let backup_path = dir.path().join("vault.db.migration.bak");

    let conn = open_encrypted_connection(&db_path, &test_key()).expect("open encrypted db");
    conn.execute("INSERT INTO tags (name) VALUES ('backup-secret-tag')", [])
        .expect("insert tag before backup");
    let result = run_with_encrypted_backup(conn, &db_path, &backup_path, &test_key(), |c| {
        c.execute("INSERT INTO tags (name) VALUES ('post-backup-tag')", [])
            .map_err(|source| MigrationError::ExecutionFailed {
                version: 99,
                name: "sqlcipher smoke test insert".to_string(),
                source,
            })?;
        Err(MigrationError::ExecutionFailed {
            version: 99,
            name: "sqlcipher smoke test".to_string(),
            source: rusqlite::Error::ExecuteReturnedResults,
        })
    });

    assert!(result.is_err(), "test migration must fail to retain backup");
    assert_plain_sqlite_schema_read_fails(&db_path);

    let restored = Connection::open(&db_path).expect("open restored db handle");
    apply_key(&restored, &test_key()).expect("apply restored db key");
    let restored_backup_secret_count: i64 = restored
        .query_row(
            "SELECT COUNT(*) FROM tags WHERE name = 'backup-secret-tag'",
            [],
            |row| row.get(0),
        )
        .expect("read restored live db");
    assert_eq!(
        restored_backup_secret_count, 1,
        "restored live DB must retain the pre-migration row"
    );
    let restored_post_backup_count: i64 = restored
        .query_row(
            "SELECT COUNT(*) FROM tags WHERE name = 'post-backup-tag'",
            [],
            |row| row.get(0),
        )
        .expect("read restored live db");
    assert_eq!(
        restored_post_backup_count, 0,
        "restored live DB must not retain failed migration writes"
    );

    assert!(
        backup_path.exists(),
        "failed migration backup path must retain a backup file"
    );
    assert_plain_sqlite_schema_read_fails(&backup_path);
    assert_file_does_not_contain(&backup_path, b"backup-secret-tag");
    assert_file_does_not_contain(&backup_path, b"post-backup-tag");

    let backup = Connection::open(&backup_path).expect("open backup handle");
    apply_key(&backup, &test_key()).expect("apply backup key");
    let backup_secret_count: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM tags WHERE name = 'backup-secret-tag'",
            [],
            |row| row.get(0),
        )
        .expect("read encrypted backup");
    assert_eq!(
        backup_secret_count, 1,
        "encrypted backup must remain readable with the original SQLCipher key"
    );
}
