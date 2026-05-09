use rusqlite::Connection;

use super::*;

fn metadata_value(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

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

#[test]
fn fresh_db_creates_all_tables() {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::schema::apply_pragmas(&conn).expect("failed to apply pragmas");
    run_migrations(&conn).unwrap();

    let expected = vec![
        "audit_log",
        "metadata",
        "password_history",
        "record_health_state",
        "record_tags",
        "records",
        "sync_state",
        "tags",
    ];
    let tables = table_names(&conn);
    assert_eq!(tables, expected);
}

#[test]
fn fresh_db_sets_schema_version_to_one() {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::schema::apply_pragmas(&conn).expect("failed to apply pragmas");
    run_migrations(&conn).unwrap();

    let version = metadata_value(&conn, "schema_version").unwrap();
    assert_eq!(version, "1");
}

#[test]
fn fresh_db_seeds_metadata() {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::schema::apply_pragmas(&conn).expect("failed to apply pragmas");
    run_migrations(&conn).unwrap();

    for key in &["vault_id", "device_id", "created_at", "current_dek_version"] {
        assert!(
            metadata_value(&conn, key).is_some(),
            "missing metadata key: {key}"
        );
    }
}

#[test]
fn run_migrations_is_idempotent() {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::schema::apply_pragmas(&conn).expect("failed to apply pragmas");

    run_migrations(&conn).unwrap();
    let version_after_first = metadata_value(&conn, "schema_version").unwrap();
    let vault_id_first = metadata_value(&conn, "vault_id").unwrap();

    run_migrations(&conn).unwrap();
    let version_after_second = metadata_value(&conn, "schema_version").unwrap();
    let vault_id_second = metadata_value(&conn, "vault_id").unwrap();

    assert_eq!(version_after_first, version_after_second);
    assert_eq!(vault_id_first, vault_id_second);
}

#[test]
fn downgrade_does_not_error() {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::schema::apply_pragmas(&conn).expect("failed to apply pragmas");

    // Set schema_version to a value higher than SCHEMA_VERSION.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO metadata (key, value) VALUES ('schema_version', '99')",
        [],
    )
    .unwrap();

    assert!(run_migrations(&conn).is_ok());

    let version = metadata_value(&conn, "schema_version").unwrap();
    assert_eq!(version, "99");
}
