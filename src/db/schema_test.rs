use rusqlite::Connection;

use super::schema::{apply_pragmas, init_db_in_memory, initialize_metadata, initialize_schema};

/// Helper: create a fully-initialized in-memory database.
fn fresh_db() -> Connection {
    init_db_in_memory()
}

/// Collect user-table names from sqlite_master, excluding SQLite internal tables
/// (prefixed with `sqlite_`).
fn table_names(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

/// Collect column names for a given table via PRAGMA.
fn column_names(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

/// Read a metadata value by key. Returns None if the key does not exist.
fn metadata_value(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

// ---------------------------------------------------------------------------
// Test 1: Schema creates all 7 tables
// ---------------------------------------------------------------------------

#[test]
fn schema_creates_all_seven_tables() {
    let db = fresh_db();
    let tables = table_names(&db);

    // sqlite internal tables are excluded — only user tables should appear.
    let expected = vec![
        "audit_log",
        "metadata",
        "password_history",
        "record_tags",
        "records",
        "sync_state",
        "tags",
    ];

    for table in &expected {
        assert!(
            tables.contains(&table.to_string()),
            "missing table: {table}"
        );
    }
    assert_eq!(
        tables.len(),
        expected.len(),
        "expected exactly {} user tables, got {:?}",
        expected.len(),
        tables
    );
}

// ---------------------------------------------------------------------------
// Test 2: records table has all required columns
// ---------------------------------------------------------------------------

#[test]
fn records_table_has_all_required_columns() {
    let db = fresh_db();
    let columns = column_names(&db, "records");

    let expected = vec![
        "id",
        "credential_type",
        "encrypted_data",
        "nonce",
        "dek_version",
        "aad",
        "is_favorite",
        "expires_at",
        "created_at",
        "updated_at",
        "updated_by",
        "version",
        "deleted",
        "deleted_at",
    ];

    for col in &expected {
        assert!(
            columns.contains(&col.to_string()),
            "records table missing column: {col}"
        );
    }
    assert_eq!(
        columns.len(),
        expected.len(),
        "records table has unexpected columns: {:?}",
        columns
    );
}

// ---------------------------------------------------------------------------
// Test 3: PRAGMAs are set correctly
// ---------------------------------------------------------------------------

#[test]
fn pragmas_are_set_correctly() {
    let db = fresh_db();

    // journal_mode: in-memory databases report "memory" instead of "wal",
    // so we accept both for correctness.
    let journal_mode: String = db
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert!(
        journal_mode == "wal" || journal_mode == "memory",
        "journal_mode should be 'wal' (or 'memory' for in-memory db), got '{journal_mode}'"
    );

    let foreign_keys: i32 = db
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        foreign_keys, 1,
        "foreign_keys pragma should be ON (1), got {foreign_keys}"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Initial metadata keys exist
// ---------------------------------------------------------------------------

#[test]
fn initial_metadata_keys_exist() {
    let db = fresh_db();

    let expected_keys = vec![
        "schema_version",
        "vault_id",
        "device_id",
        "created_at",
        "current_dek_version",
    ];

    for key in &expected_keys {
        assert!(
            metadata_value(&db, key).is_some(),
            "metadata missing key: {key}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 5: Schema version is "2"
// ---------------------------------------------------------------------------

#[test]
fn schema_version_is_two() {
    let db = fresh_db();

    let version = metadata_value(&db, "schema_version").unwrap();
    assert_eq!(
        version, "2",
        "schema_version should be \"2\", got \"{version}\""
    );
}

// ---------------------------------------------------------------------------
// Test 6: Initialization is idempotent
// ---------------------------------------------------------------------------

#[test]
fn initialization_is_idempotent() {
    let conn = Connection::open_in_memory().unwrap();
    apply_pragmas(&conn);

    // First initialization.
    initialize_schema(&conn);
    initialize_metadata(&conn);

    let tables_after_first = table_names(&conn);
    let vault_id_first = metadata_value(&conn, "vault_id").unwrap();

    // Second initialization — should not panic or change state.
    initialize_schema(&conn);
    initialize_metadata(&conn);

    let tables_after_second = table_names(&conn);
    let vault_id_second = metadata_value(&conn, "vault_id").unwrap();

    // Tables unchanged.
    assert_eq!(tables_after_first, tables_after_second);

    // Metadata preserved (INSERT OR IGNORE keeps original values).
    assert_eq!(vault_id_first, vault_id_second);
}

// ---------------------------------------------------------------------------
// Test 7: Cascade delete works (record -> record_tags)
// ---------------------------------------------------------------------------

#[test]
fn cascade_delete_removes_record_tags() {
    let db = fresh_db();

    // Insert a minimal record.
    db.execute(
        "INSERT INTO records (id, credential_type, encrypted_data, nonce, created_at, updated_at, updated_by)
         VALUES ('rec-1', 'login', X'DEAD', X'BEEF', 1000, 1000, 'test')",
        [],
    )
    .unwrap();

    // Insert a tag.
    db.execute("INSERT INTO tags (name) VALUES ('work')", [])
        .unwrap();

    // Link record to tag.
    db.execute(
        "INSERT INTO record_tags (record_id, tag_id) VALUES ('rec-1', 1)",
        [],
    )
    .unwrap();

    // Verify the link exists.
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM record_tags WHERE record_id = 'rec-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "record_tags should contain 1 row before delete");

    // Delete the record — should cascade to record_tags.
    db.execute("DELETE FROM records WHERE id = 'rec-1'", [])
        .unwrap();

    let count_after: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM record_tags WHERE record_id = 'rec-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count_after, 0,
        "record_tags should be empty after cascade delete"
    );
}
