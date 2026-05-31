use rusqlite::Connection;

use super::schema::init_db_in_memory;

/// Helper: create a fully-initialized in-memory database.
fn fresh_db() -> Connection {
    init_db_in_memory().unwrap()
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
// Test 1: Schema creates all 9 tables
// ---------------------------------------------------------------------------

#[test]
fn schema_creates_all_nine_tables() {
    let db = fresh_db();
    let tables = table_names(&db);

    // sqlite internal tables are excluded — only user tables should appear.
    let expected = vec![
        "audit_log",
        "metadata",
        "password_history",
        "record_health_state",
        "record_list_index",
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

#[test]
fn record_list_index_table_has_required_columns() {
    let db = fresh_db();
    let columns = column_names(&db, "record_list_index");

    let expected = vec![
        "record_id",
        "name",
        "subtitle",
        "search_text",
        "name_sort_key",
    ];

    for col in &expected {
        assert!(
            columns.contains(&col.to_string()),
            "record_list_index missing column: {col}"
        );
    }
    assert_eq!(
        columns.len(),
        expected.len(),
        "record_list_index has unexpected columns: {:?}",
        columns
    );
}

// ---------------------------------------------------------------------------
// Test 6: Initialization is idempotent
// ---------------------------------------------------------------------------

#[test]
fn initialization_is_idempotent() {
    let conn = init_db_in_memory().unwrap();

    let tables_after_first = table_names(&conn);
    let vault_id_first = metadata_value(&conn, "vault_id").unwrap();

    crate::db::migrations::run_migrations(&conn).unwrap();

    let tables_after_second = table_names(&conn);
    let vault_id_second = metadata_value(&conn, "vault_id").unwrap();

    assert_eq!(tables_after_first, tables_after_second);
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

// ---------------------------------------------------------------------------
// Test 8: Cascade delete works (record -> password_history)
// ---------------------------------------------------------------------------

#[test]
fn cascade_delete_removes_password_history() {
    let db = fresh_db();

    // Insert a minimal record.
    db.execute(
        "INSERT INTO records (id, credential_type, encrypted_data, nonce, created_at, updated_at, updated_by)
         VALUES ('rec-2', 'login', X'DEAD', X'BEEF', 1000, 1000, 'test')",
        [],
    )
    .unwrap();

    // Insert password history entries.
    db.execute(
        "INSERT INTO password_history (record_id, encrypted_password, nonce, changed_at)
         VALUES ('rec-2', X'CAFE', X'BEEF', 2000)",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO password_history (record_id, encrypted_password, nonce, changed_at)
         VALUES ('rec-2', X'FACE', X'DEAD', 3000)",
        [],
    )
    .unwrap();

    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM password_history WHERE record_id = 'rec-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 2,
        "password_history should contain 2 rows before delete"
    );

    // Delete the record — should cascade to password_history.
    db.execute("DELETE FROM records WHERE id = 'rec-2'", [])
        .unwrap();

    let count_after: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM password_history WHERE record_id = 'rec-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count_after, 0,
        "password_history should be empty after cascade delete"
    );
}

// ---------------------------------------------------------------------------
// Test 9: Cascade delete works (record -> sync_state)
// ---------------------------------------------------------------------------

#[test]
fn cascade_delete_removes_sync_state() {
    let db = fresh_db();

    // Insert a minimal record.
    db.execute(
        "INSERT INTO records (id, credential_type, encrypted_data, nonce, created_at, updated_at, updated_by)
         VALUES ('rec-3', 'login', X'DEAD', X'BEEF', 1000, 1000, 'test')",
        [],
    )
    .unwrap();

    // Insert sync state.
    db.execute(
        "INSERT INTO sync_state (record_id, local_updated_at, sync_status)
         VALUES ('rec-3', 2000, 0)",
        [],
    )
    .unwrap();

    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sync_state WHERE record_id = 'rec-3'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "sync_state should contain 1 row before delete");

    // Delete the record — should cascade to sync_state.
    db.execute("DELETE FROM records WHERE id = 'rec-3'", [])
        .unwrap();

    let count_after: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sync_state WHERE record_id = 'rec-3'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count_after, 0,
        "sync_state should be empty after cascade delete"
    );
}

// ---------------------------------------------------------------------------
// Test 10: record_health_state table has all required columns
// ---------------------------------------------------------------------------

#[test]
fn record_health_state_table_has_all_required_columns() {
    let db = fresh_db();
    let columns = column_names(&db, "record_health_state");

    let expected = vec![
        "record_id",
        "record_version",
        "evaluated_at",
        "weak_password",
        "duplicate_group_size",
        "compromised",
        "expired",
    ];

    for col in &expected {
        assert!(
            columns.contains(&col.to_string()),
            "record_health_state table missing column: {col}"
        );
    }
    assert_eq!(
        columns.len(),
        expected.len(),
        "record_health_state table has unexpected columns: {:?}",
        columns
    );
}

// ---------------------------------------------------------------------------
// Test 11: Cascade delete works (record -> record_health_state)
// ---------------------------------------------------------------------------

#[test]
fn cascade_delete_removes_record_health_state() {
    let db = fresh_db();

    // Insert a minimal record.
    db.execute(
        "INSERT INTO records (id, credential_type, encrypted_data, nonce, created_at, updated_at, updated_by)
         VALUES ('rec-hs', 'login', X'DEAD', X'BEEF', 1000, 1000, 'test')",
        [],
    )
    .unwrap();

    // Insert health state.
    db.execute(
        "INSERT INTO record_health_state
            (record_id, record_version, evaluated_at, weak_password,
             duplicate_group_size, compromised, expired)
         VALUES ('rec-hs', 1, 1700000000, 1, 3, 0, NULL)",
        [],
    )
    .unwrap();

    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM record_health_state WHERE record_id = 'rec-hs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 1,
        "record_health_state should contain 1 row before delete"
    );

    // Delete the record — should cascade to record_health_state.
    db.execute("DELETE FROM records WHERE id = 'rec-hs'", [])
        .unwrap();

    let count_after: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM record_health_state WHERE record_id = 'rec-hs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count_after, 0,
        "record_health_state should be empty after cascade delete"
    );
}
