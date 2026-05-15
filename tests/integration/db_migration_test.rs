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
    let (_dir, path) = setup_fresh_db();

    let version_before: String = {
        let conn = rusqlite::Connection::open(path.join("vault.db")).unwrap();
        oak_keyring::db::schema::apply_pragmas(&conn).unwrap();
        conn.query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };

    let conn = oak_keyring::db::schema::init_db(&path).unwrap();
    let version_after: String = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    conn.close().unwrap();

    assert_eq!(
        version_before, version_after,
        "schema_version should not change when already current"
    );
    assert!(!path.join("vault.db.migration.bak").exists());
}

#[test]
fn init_db_creates_fresh_db_from_empty_file() {
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
fn init_db_rejects_existing_corrupt_database() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("vault.db"), b"not a sqlite database").unwrap();

    let err = oak_keyring::db::schema::init_db(dir.path()).unwrap_err();
    match err {
        oak_keyring::db::schema::InitDbError::Sqlite(source) => {
            let message = source.to_string();
            assert!(
                message.contains("file is not a database")
                    || message.contains("database disk image is malformed"),
                "unexpected sqlite error: {message}"
            );
        }
        other => panic!("expected sqlite error for corrupt database, got {other:?}"),
    }
}

#[test]
fn init_db_rejects_current_schema_database_with_foreign_key_violation() {
    let (_dir, path) = setup_fresh_db();
    {
        let conn = rusqlite::Connection::open(path.join("vault.db")).unwrap();
        oak_keyring::db::schema::apply_pragmas(&conn).unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys=OFF;
             INSERT INTO record_health_state
                (record_id, record_version, evaluated_at, weak_password, duplicate_group_size, compromised, expired)
             VALUES
                ('missing-record', 1, NULL, NULL, NULL, NULL, NULL);",
        )
        .unwrap();
        conn.close().unwrap();
    }

    let err = oak_keyring::db::schema::init_db(&path).unwrap_err();
    match err {
        oak_keyring::db::schema::InitDbError::ForeignKeyCheckFailed(message) => {
            assert!(
                message.contains("table=record_health_state"),
                "unexpected foreign key check message: {message}"
            );
            assert!(
                message.contains("parent=records"),
                "unexpected foreign key check message: {message}"
            );
        }
        other => panic!("expected foreign key check error, got {other:?}"),
    }
}

#[test]
fn init_db_rejects_newer_schema_version() {
    let dir = tempfile::tempdir().unwrap();

    // Create a database at version 99 — newer than current SCHEMA_VERSION.
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

    let err = oak_keyring::db::schema::init_db(dir.path()).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("newer than this build supports"),
        "unexpected error: {message}"
    );
}

#[test]
fn checkpoint_wal_succeeds_for_file_backed_database() {
    let dir = tempfile::tempdir().unwrap();
    let conn = oak_keyring::db::schema::init_db(dir.path()).unwrap();

    conn.execute("INSERT INTO tags (name) VALUES ('shutdown-test')", [])
        .unwrap();

    oak_keyring::db::schema::checkpoint_wal(&conn).unwrap();
    conn.close().unwrap();
}

// ---------------------------------------------------------------------------
// Backup / restore orchestration tests
// ---------------------------------------------------------------------------
//
// These exercise `run_with_backup` directly. The trigger inside `init_db`
// (`needs_migration && current > 0`) is unreachable while SCHEMA_VERSION = 1,
// so the orchestration logic is verified independently of that gate.

/// Drives `run_with_backup` with a closure that mutates the database and then
/// fails. Verifies:
/// - the pre-mutation state is restored from the backup
/// - WAL-resident commits made before the call ARE captured by the backup
///   (regression guard for the prior `std::fs::copy(vault.db)` implementation,
///   which would have lost commits sitting in `vault.db-wal`)
/// - the backup file is retained on failure for forensics
#[test]
fn run_with_backup_restores_wal_resident_data_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("vault.db");
    let backup_path = dir.path().join("vault.db.bak");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    oak_keyring::db::schema::apply_pragmas(&conn).unwrap();
    conn.execute_batch(
        "CREATE TABLE marker (value TEXT NOT NULL);
         INSERT INTO marker VALUES ('original');",
    )
    .unwrap();

    // 'original' is committed but, in WAL mode without a checkpoint, lives in
    // vault.db-wal — not in vault.db. A naive std::fs::copy of vault.db would
    // miss it.
    let result = oak_keyring::db::schema::run_with_backup(conn, &db_path, &backup_path, |c| {
        c.execute("INSERT INTO marker VALUES ('mutated')", [])
            .unwrap();
        Err(
            oak_keyring::db::migrations::MigrationError::ExecutionFailed {
                version: 99,
                name: "test_failure".to_string(),
                source: rusqlite::Error::ExecuteReturnedResults,
            },
        )
    });

    assert!(result.is_err(), "work failure should propagate");
    assert!(
        backup_path.exists(),
        "backup file should be retained for forensics after restore"
    );

    // Re-open the restored database. 'original' must still be present (proves
    // the backup captured WAL-resident data); 'mutated' must be absent (proves
    // the failed work was rolled back via restore).
    let reopened = rusqlite::Connection::open(&db_path).unwrap();
    let original: i64 = reopened
        .query_row(
            "SELECT COUNT(*) FROM marker WHERE value = 'original'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        original, 1,
        "WAL-resident 'original' row should survive restore"
    );

    let mutated: i64 = reopened
        .query_row(
            "SELECT COUNT(*) FROM marker WHERE value = 'mutated'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(mutated, 0, "post-failure mutation must be rolled back");
}

#[test]
fn run_with_backup_removes_backup_on_success() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("vault.db");
    let backup_path = dir.path().join("vault.db.bak");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    oak_keyring::db::schema::apply_pragmas(&conn).unwrap();
    conn.execute_batch("CREATE TABLE x (y INTEGER NOT NULL);")
        .unwrap();

    let conn = oak_keyring::db::schema::run_with_backup(conn, &db_path, &backup_path, |_c| Ok(()))
        .unwrap();

    assert!(
        !backup_path.exists(),
        "backup should be cleaned up on successful work"
    );
    drop(conn);
}

#[test]
fn run_with_backup_overwrites_stale_backup_file() {
    // Defensive: a leftover .bak from a prior incomplete run must not corrupt
    // a fresh backup. backup_database removes the stale file before snapshotting.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("vault.db");
    let backup_path = dir.path().join("vault.db.bak");

    std::fs::write(&backup_path, b"garbage from a prior run").unwrap();

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    oak_keyring::db::schema::apply_pragmas(&conn).unwrap();
    conn.execute_batch("CREATE TABLE t (v INTEGER);").unwrap();

    let result = oak_keyring::db::schema::run_with_backup(conn, &db_path, &backup_path, |_c| {
        Err(
            oak_keyring::db::migrations::MigrationError::ExecutionFailed {
                version: 99,
                name: "test".to_string(),
                source: rusqlite::Error::ExecuteReturnedResults,
            },
        )
    });

    assert!(result.is_err());
    // The .bak file is now a real SQLite database (not the garbage), and the
    // restored vault.db opens cleanly with the expected schema.
    let reopened = rusqlite::Connection::open(&db_path).unwrap();
    let count: i64 = reopened
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='t'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "restored database should retain pre-work schema");
}
