use chrono::{TimeZone, Utc};
use rusqlite::Connection;
use uuid::Uuid;

use super::queries::*;
use super::schema::init_db_in_memory;
use crate::types::credential::CredentialType;
use crate::types::record::StoredRecord;

fn fresh_db() -> Connection {
    init_db_in_memory()
}

/// Build a minimal `StoredRecord` with sensible defaults for testing.
fn sample_record(id: Uuid) -> StoredRecord {
    StoredRecord {
        id,
        credential_type: CredentialType::Login,
        encrypted_data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        nonce: [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x10, 0x11, 0x12, 0x13,
            0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x20, 0x21, 0x22, 0x23,
        ],
        dek_version: 2,
        aad: vec![0xAA, 0xDD],
        is_favorite: true,
        expires_at: Some(Utc.timestamp_opt(1800000000, 0).single().unwrap()),
        created_at: Utc.timestamp_opt(1700000000, 0).single().unwrap(),
        updated_at: Utc.timestamp_opt(1700000100, 0).single().unwrap(),
        updated_by: "device-test".to_string(),
        version: 3,
        deleted: false,
        deleted_at: None,
        tags: vec!["work".to_string(), "important".to_string()],
    }
}

// ---------------------------------------------------------------------------
// Test 1: Insert then get, verify all fields round-trip correctly
// ---------------------------------------------------------------------------

#[test]
fn insert_and_get_record() {
    let db = fresh_db();
    let id = Uuid::new_v4();
    let original = sample_record(id);

    insert_record(&db, &original).unwrap();

    let fetched = get_record(&db, &id).unwrap().expect("record should exist");

    assert_eq!(fetched.id, original.id);
    assert_eq!(fetched.credential_type, original.credential_type);
    assert_eq!(fetched.encrypted_data, original.encrypted_data);
    assert_eq!(fetched.nonce, original.nonce);
    assert_eq!(fetched.dek_version, original.dek_version);
    assert_eq!(fetched.aad, original.aad);
    assert_eq!(fetched.is_favorite, original.is_favorite);
    assert_eq!(fetched.expires_at, original.expires_at);
    assert_eq!(fetched.created_at, original.created_at);
    assert_eq!(fetched.updated_at, original.updated_at);
    assert_eq!(fetched.updated_by, original.updated_by);
    assert_eq!(fetched.version, original.version);
    assert!(!fetched.deleted);
    assert!(fetched.deleted_at.is_none());
    assert_eq!(
        fetched.tags,
        vec!["work".to_string(), "important".to_string()]
    );
}

// ---------------------------------------------------------------------------
// Test 2: Get with a random UUID returns None
// ---------------------------------------------------------------------------

#[test]
fn get_nonexistent_record_returns_none() {
    let db = fresh_db();

    let result = get_record(&db, &Uuid::new_v4()).unwrap();
    assert!(
        result.is_none(),
        "should return None for nonexistent record"
    );
}

// ---------------------------------------------------------------------------
// Test 3: list_active_records excludes soft-deleted records
// ---------------------------------------------------------------------------

#[test]
fn list_records_excludes_soft_deleted() {
    let db = fresh_db();

    let id_active = Uuid::new_v4();
    let id_deleted = Uuid::new_v4();

    let mut active = sample_record(id_active);
    active.tags = vec!["active-tag".to_string()];
    insert_record(&db, &active).unwrap();

    let mut deleted = sample_record(id_deleted);
    deleted.tags = vec!["deleted-tag".to_string()];
    insert_record(&db, &deleted).unwrap();

    // Soft-delete the second record
    soft_delete_record(&db, &id_deleted).unwrap();

    let records = list_active_records(&db).unwrap();
    assert_eq!(records.len(), 1, "only one active record expected");
    assert_eq!(records[0].id, id_active);
    assert_eq!(records[0].tags, vec!["active-tag".to_string()]);
}

// ---------------------------------------------------------------------------
// Test 4: soft-delete then restore, verify flags change correctly
// ---------------------------------------------------------------------------

#[test]
fn soft_delete_and_restore() {
    let db = fresh_db();
    let id = Uuid::new_v4();

    let mut record = sample_record(id);
    record.tags = vec!["restore-test".to_string()];
    insert_record(&db, &record).unwrap();

    // Soft delete
    soft_delete_record(&db, &id).unwrap();
    let deleted = get_record(&db, &id).unwrap().expect("should still exist");
    assert!(deleted.deleted, "record should be marked deleted");
    assert!(deleted.deleted_at.is_some(), "deleted_at should be set");

    // Restore
    restore_record(&db, &id).unwrap();
    let restored = get_record(&db, &id).unwrap().expect("should still exist");
    assert!(!restored.deleted, "record should no longer be deleted");
    assert!(
        restored.deleted_at.is_none(),
        "deleted_at should be cleared"
    );
    assert_eq!(restored.tags, vec!["restore-test".to_string()]);
}

// ---------------------------------------------------------------------------
// Test 5: hard-delete removes the record entirely
// ---------------------------------------------------------------------------

#[test]
fn hard_delete_removes_record() {
    let db = fresh_db();
    let id = Uuid::new_v4();

    let mut record = sample_record(id);
    record.tags = vec!["doomed".to_string()];
    insert_record(&db, &record).unwrap();

    // Confirm it exists before deletion
    assert!(get_record(&db, &id).unwrap().is_some());

    hard_delete_record(&db, &id).unwrap();

    let result = get_record(&db, &id).unwrap();
    assert!(result.is_none(), "record should be gone after hard delete");
}
