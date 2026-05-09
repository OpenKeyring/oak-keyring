use std::path::PathBuf;
use tempfile::TempDir;

/// Helper: create a fresh temp directory with a vault.db via init_db.
fn setup_fresh_db() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let conn = oak_keyring::db::schema::init_db(&path).unwrap();
    conn.close().unwrap();
    (dir, path)
}

#[test]
fn init_db_creates_working_database_on_fresh_directory() {
    let dir = tempfile::tempdir().unwrap();
    let conn = oak_keyring::db::schema::init_db(dir.path()).unwrap();

    let version: String = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, "1");
    conn.close().unwrap();

    assert!(!dir.path().join("vault.db.migration.bak").exists());
}

#[test]
fn init_db_on_existing_current_version_is_no_op() {
    let (dir, path) = setup_fresh_db();

    let mtime_before = std::fs::metadata(path.join("vault.db"))
        .unwrap()
        .modified()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(50));

    let conn = oak_keyring::db::schema::init_db(&path).unwrap();
    conn.close().unwrap();

    let mtime_after = std::fs::metadata(path.join("vault.db"))
        .unwrap()
        .modified()
        .unwrap();

    assert_eq!(
        mtime_before, mtime_after,
        "database should not be modified when already at current version"
    );
    assert!(!path.join("vault.db.migration.bak").exists());
}

#[test]
fn init_db_removes_backup_after_successful_migration() {
    let dir = tempfile::tempdir().unwrap();

    // Create an empty database (version 0) — no metadata table.
    {
        let conn = rusqlite::Connection::open(dir.path().join("vault.db")).unwrap();
        oak_keyring::db::schema::apply_pragmas(&conn).unwrap();
        conn.close().unwrap();
    }

    // Run init_db — should trigger migration without backup (version 0 = fresh).
    let conn = oak_keyring::db::schema::init_db(dir.path()).unwrap();
    let version: String = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, "1");
    conn.close().unwrap();

    assert!(!dir.path().join("vault.db.migration.bak").exists());
}

#[test]
fn init_db_restores_backup_on_migration_failure() {
    let dir = tempfile::tempdir().unwrap();

    // Create a database at version 99 — downgrade path.
    {
        let conn = rusqlite::Connection::open(dir.path().join("vault.db")).unwrap();
        oak_keyring::db::schema::apply_pragmas(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO metadata (key, value) VALUES ('schema_version', '99');",
        )
        .unwrap();
        conn.close().unwrap();
    }

    // init_db should succeed — downgrade is warn + continue.
    let conn = oak_keyring::db::schema::init_db(dir.path()).unwrap();
    let version: String = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, "99");
    conn.close().unwrap();
}
