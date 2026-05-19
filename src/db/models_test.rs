use rusqlite::Connection;

use super::models::{AuditLogRow, RecordRow, SyncStateRow, TagRow};
use super::schema::init_db_in_memory;

fn fresh_db() -> Connection {
    init_db_in_memory().unwrap()
}

/// Helper: insert a minimal record with all fields populated.
fn insert_full_record(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO records
            (id, credential_type, encrypted_data, nonce, dek_version, aad,
             is_favorite, expires_at, created_at, updated_at, updated_by,
             version, deleted, deleted_at)
         VALUES
            (?1, 'login', X'DEADBEEF', X'000102030405060708091011121314151617181920212223',
             2, X'AADDAA44', 1, 1700000000, 1700000000, 1700000100, 'device-1',
             3, 0, NULL)",
        rusqlite::params![id],
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Test 1: RecordRow converts all fields correctly
// ---------------------------------------------------------------------------

#[test]
fn record_row_from_db_converts_to_stored_record() {
    let db = fresh_db();
    insert_full_record(&db, "550e8400-e29b-41d4-a716-446655440000");

    let row: RecordRow = db
        .query_row(
            "SELECT * FROM records WHERE id = ?1",
            rusqlite::params!["550e8400-e29b-41d4-a716-446655440000"],
            RecordRow::from_row,
        )
        .unwrap();

    let record = row.to_stored_record(vec!["work".to_string()]).unwrap();

    assert_eq!(
        record.id.to_string(),
        "550e8400-e29b-41d4-a716-446655440000"
    );
    assert_eq!(
        record.credential_type,
        crate::types::credential::CredentialType::Login
    );
    assert_eq!(record.encrypted_data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(
        record.nonce,
        [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x10, 0x11, 0x12, 0x13,
            0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x20, 0x21, 0x22, 0x23
        ]
    );
    assert_eq!(record.dek_version, 2);
    assert_eq!(record.aad, vec![0xAA, 0xDD, 0xAA, 0x44]);
    assert!(record.is_favorite);
    assert!(record.expires_at.is_some());
    assert!(!record.deleted);
    assert!(record.deleted_at.is_none());
    assert_eq!(record.version, 3);
    assert_eq!(record.updated_by, "device-1");
    assert_eq!(record.tags, vec!["work".to_string()]);
}

// ---------------------------------------------------------------------------
// Test 2: RecordRow handles nullable fields (NULL aad, expires_at, deleted_at)
// ---------------------------------------------------------------------------

#[test]
fn record_row_handles_nullable_fields() {
    let db = fresh_db();

    // Insert with NULLs for optional fields.
    db.execute(
        "INSERT INTO records
            (id, credential_type, encrypted_data, nonce, dek_version, aad,
             is_favorite, expires_at, created_at, updated_at, updated_by,
             version, deleted, deleted_at)
         VALUES
            ('11111111-1111-1111-1111-111111111111', 'api',
             X'CAFE', X'000102030405060708091011121314151617181920212223',
             1, NULL, 0, NULL, 1700000000, 1700000000, 'dev',
             1, 0, NULL)",
        [],
    )
    .unwrap();

    let row: RecordRow = db
        .query_row(
            "SELECT * FROM records WHERE id = '11111111-1111-1111-1111-111111111111'",
            [],
            RecordRow::from_row,
        )
        .unwrap();

    let record = row.to_stored_record(vec![]).unwrap();

    assert!(
        record.aad.is_empty(),
        "aad should default to empty vec when NULL"
    );
    assert!(
        record.expires_at.is_none(),
        "expires_at should be None when NULL"
    );
    assert!(
        record.deleted_at.is_none(),
        "deleted_at should be None when NULL"
    );
    assert!(!record.is_favorite, "is_favorite should be false when 0");
    assert!(!record.deleted, "deleted should be false when 0");
    assert!(record.tags.is_empty(), "tags should be empty");
}

// ---------------------------------------------------------------------------
// Test 3: TagRow reads from tags table
// ---------------------------------------------------------------------------

#[test]
fn tag_row_from_db() {
    let db = fresh_db();

    db.execute("INSERT INTO tags (name) VALUES ('work')", [])
        .unwrap();

    let row: TagRow = db
        .query_row(
            "SELECT * FROM tags WHERE name = 'work'",
            [],
            TagRow::from_row,
        )
        .unwrap();

    let tag = row.to_tag();

    assert_eq!(tag.id, 1);
    assert_eq!(tag.name, "work");
}

// ---------------------------------------------------------------------------
// Test 4: AuditLogRow converts all fields
// ---------------------------------------------------------------------------

#[test]
fn audit_log_row_from_db() {
    let db = fresh_db();

    db.execute(
        "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at)
         VALUES ('record.create', '550e8400-e29b-41d4-a716-446655440000', 'My Login', NULL, 1700000500)",
        [],
    )
    .unwrap();

    let row: AuditLogRow = db
        .query_row(
            "SELECT * FROM audit_log WHERE operation = 'record.create'",
            [],
            AuditLogRow::from_row,
        )
        .unwrap();

    let entry = row.to_audit_entry().unwrap();

    assert_eq!(entry.id, 1);
    assert_eq!(
        entry.operation,
        crate::types::audit::AuditOperation::RecordCreate
    );
    assert!(entry.record_id.is_some());
    assert_eq!(entry.record_name.as_deref(), Some("My Login"));
    assert!(entry.detail.is_none());
}

// ---------------------------------------------------------------------------
// Test 5: SyncStateRow converts all fields
// ---------------------------------------------------------------------------

#[test]
fn sync_state_row_from_db() {
    let db = fresh_db();

    // Need a record first due to FK constraint.
    db.execute(
        "INSERT INTO records
            (id, credential_type, encrypted_data, nonce, created_at, updated_at, updated_by)
         VALUES
            ('550e8400-e29b-41d4-a716-446655440000', 'login', X'AA', X'000102030405060708091011121314151617181920212223',
             1700000000, 1700000000, 'dev')",
        [],
    )
    .unwrap();

    db.execute(
        "INSERT INTO sync_state (record_id, cloud_updated_at, local_updated_at, sync_status, conflict_data)
         VALUES ('550e8400-e29b-41d4-a716-446655440000', 1700000100, 1700000200, 1, NULL)",
        [],
    )
    .unwrap();

    let row: SyncStateRow = db
        .query_row(
            "SELECT * FROM sync_state WHERE record_id = '550e8400-e29b-41d4-a716-446655440000'",
            [],
            SyncStateRow::from_row,
        )
        .unwrap();

    let state = row.to_sync_state().unwrap();

    assert_eq!(
        state.record_id.to_string(),
        "550e8400-e29b-41d4-a716-446655440000"
    );
    assert!(state.cloud_updated_at.is_some());
    assert_eq!(state.sync_status, crate::types::sync::SyncStatus::Synced);
    assert!(state.conflict_data.is_none());
}
