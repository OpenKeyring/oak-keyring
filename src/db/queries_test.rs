use chrono::{TimeZone, Utc};
use rusqlite::Connection;
use uuid::Uuid;

use super::queries::*;
use super::schema::init_db_in_memory;
use crate::types::audit::AuditOperation;
use crate::types::credential::CredentialType;
use crate::types::health::RecordHealthState;
use crate::types::record::StoredRecord;

fn fresh_db() -> Connection {
    init_db_in_memory().unwrap()
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

// ---------------------------------------------------------------------------
// Test 6: Tag CRUD operations
// ---------------------------------------------------------------------------

#[test]
fn tag_crud() {
    let db = fresh_db();

    // Insert a new tag
    let tag = insert_tag(&db, "rust").unwrap();
    assert_eq!(tag.name, "rust");
    assert!(tag.id > 0);

    // get_or_create with same name returns existing tag
    let existing = get_or_create_tag(&db, "rust").unwrap();
    assert_eq!(existing.id, tag.id, "should return same tag ID");
    assert_eq!(existing.name, "rust");

    // get_or_create with a new name creates it
    let new_tag = get_or_create_tag(&db, "cli").unwrap();
    assert_ne!(new_tag.id, tag.id);
    assert_eq!(new_tag.name, "cli");

    // list_tags returns both, ordered alphabetically
    let tags = list_tags(&db).unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].name, "cli");
    assert_eq!(tags[1].name, "rust");
}

// ---------------------------------------------------------------------------
// Test 7: Attach and detach tags on a record
// ---------------------------------------------------------------------------

#[test]
fn attach_and_detach_tag() {
    let db = fresh_db();
    let id = Uuid::new_v4();

    let mut record = sample_record(id);
    record.tags = vec![];
    insert_record(&db, &record).unwrap();

    // Create a tag and attach it
    let tag = insert_tag(&db, "personal").unwrap();
    attach_tag(&db, &id, tag.id).unwrap();

    let tags = get_record_tags(&db, &id).unwrap();
    assert_eq!(tags, vec!["personal".to_string()]);

    // Detach the tag
    detach_tag(&db, &id, tag.id).unwrap();
    let tags = get_record_tags(&db, &id).unwrap();
    assert!(tags.is_empty(), "tags should be empty after detach");
}

// ---------------------------------------------------------------------------
// Test 8: Audit log insert and list
// ---------------------------------------------------------------------------

#[test]
fn audit_log_insert_and_list() {
    let db = fresh_db();
    let record_id = Uuid::new_v4();

    insert_audit_entry(
        &db,
        AuditOperation::RecordCreate,
        Some(&record_id),
        Some("my-login"),
        Some("created via TUI"),
    )
    .unwrap();

    insert_audit_entry(&db, AuditOperation::VaultUnlock, None, None, None).unwrap();

    let entries = list_audit_entries(&db, 10, 0).unwrap();
    assert_eq!(entries.len(), 2);

    // Both entries may share the same timestamp, so ordering among equal
    // occurred_at values follows SQLite rowid (insertion order). Find each
    // entry by its operation type rather than assuming index order.
    let create_entry = entries
        .iter()
        .find(|e| e.operation == AuditOperation::RecordCreate)
        .expect("RecordCreate entry should exist");
    assert_eq!(create_entry.record_id, Some(record_id));
    assert_eq!(create_entry.record_name.as_deref(), Some("my-login"));
    assert_eq!(create_entry.detail.as_deref(), Some("created via TUI"));

    let unlock_entry = entries
        .iter()
        .find(|e| e.operation == AuditOperation::VaultUnlock)
        .expect("VaultUnlock entry should exist");
    assert!(unlock_entry.record_id.is_none());
}

// ---------------------------------------------------------------------------
// Test 9: Metadata get and set
// ---------------------------------------------------------------------------

#[test]
fn metadata_get_set() {
    let db = fresh_db();

    // schema_version is seeded by migration system
    let version = get_metadata(&db, "schema_version").unwrap();
    assert_eq!(version.as_deref(), Some("1"));

    // Set a new key
    set_metadata(&db, "custom_key", "hello").unwrap();
    let val = get_metadata(&db, "custom_key").unwrap();
    assert_eq!(val.as_deref(), Some("hello"));

    // Overwrite existing key
    set_metadata(&db, "custom_key", "world").unwrap();
    let val = get_metadata(&db, "custom_key").unwrap();
    assert_eq!(val.as_deref(), Some("world"));

    // Nonexistent key returns None
    let missing = get_metadata(&db, "no_such_key").unwrap();
    assert!(missing.is_none());
}

// ---------------------------------------------------------------------------
// Health state helpers
// ---------------------------------------------------------------------------

/// Build a test `StoredRecord` with minimal defaults (no tags).
fn make_test_record(id: &Uuid, version: u64) -> StoredRecord {
    StoredRecord {
        id: *id,
        credential_type: CredentialType::Login,
        encrypted_data: vec![1, 2, 3, 4],
        nonce: [0u8; 24],
        dek_version: 1,
        aad: vec![],
        is_favorite: false,
        expires_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        updated_by: "test".to_string(),
        version,
        deleted: false,
        deleted_at: None,
        tags: vec![],
    }
}

fn make_health_state(record_id: &Uuid, record_version: u64) -> RecordHealthState {
    RecordHealthState {
        record_id: *record_id,
        record_version,
        evaluated_at: Some(Utc.timestamp_opt(1700000000, 0).single().unwrap()),
        weak_password: Some(true),
        duplicate_group_size: Some(3),
        compromised: Some(false),
        expired: Some(true),
    }
}

// ---------------------------------------------------------------------------
// Health state query tests
// ---------------------------------------------------------------------------

#[test]
fn upsert_and_get_record_health_state() {
    let db = fresh_db();
    let id = Uuid::new_v4();
    insert_record(&db, &make_test_record(&id, 1)).unwrap();

    let state = make_health_state(&id, 1);
    upsert_record_health_state(&db, &state).unwrap();

    let fetched = get_record_health_state(&db, &id).unwrap().unwrap();
    assert_eq!(fetched.record_id, id);
    assert_eq!(fetched.record_version, 1);
    assert_eq!(fetched.weak_password, Some(true));
    assert_eq!(fetched.duplicate_group_size, Some(3));
    assert_eq!(fetched.compromised, Some(false));
    assert_eq!(fetched.expired, Some(true));
    assert!(fetched.evaluated_at.is_some());
}

#[test]
fn get_record_health_state_returns_none_when_not_found() {
    let db = fresh_db();
    let id = Uuid::new_v4();
    let result = get_record_health_state(&db, &id).unwrap();
    assert!(result.is_none());
}

#[test]
fn upsert_record_health_state_updates_existing() {
    let db = fresh_db();
    let id = Uuid::new_v4();
    insert_record(&db, &make_test_record(&id, 1)).unwrap();

    let mut state = make_health_state(&id, 1);
    upsert_record_health_state(&db, &state).unwrap();

    // Update the state
    state.weak_password = Some(false);
    state.record_version = 2;
    upsert_record_health_state(&db, &state).unwrap();

    let fetched = get_record_health_state(&db, &id).unwrap().unwrap();
    assert_eq!(fetched.weak_password, Some(false));
    assert_eq!(fetched.record_version, 2);
}

#[test]
fn list_record_health_states_returns_all() {
    let db = fresh_db();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    insert_record(&db, &make_test_record(&id1, 1)).unwrap();
    insert_record(&db, &make_test_record(&id2, 1)).unwrap();

    upsert_record_health_state(&db, &make_health_state(&id1, 1)).unwrap();
    upsert_record_health_state(&db, &make_health_state(&id2, 1)).unwrap();

    let states = list_record_health_states(&db).unwrap();
    assert_eq!(states.len(), 2);

    let ids: Vec<Uuid> = states.iter().map(|s| s.record_id).collect();
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));
}

#[test]
fn list_record_health_states_returns_empty_when_none() {
    let db = fresh_db();
    let states = list_record_health_states(&db).unwrap();
    assert!(states.is_empty());
}

#[test]
fn replace_record_health_states_swaps_all() {
    let db = fresh_db();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    insert_record(&db, &make_test_record(&id1, 1)).unwrap();
    insert_record(&db, &make_test_record(&id2, 1)).unwrap();
    insert_record(&db, &make_test_record(&id3, 1)).unwrap();

    // Insert initial states for id1 and id2
    upsert_record_health_state(&db, &make_health_state(&id1, 1)).unwrap();
    upsert_record_health_state(&db, &make_health_state(&id2, 1)).unwrap();
    assert_eq!(list_record_health_states(&db).unwrap().len(), 2);

    // Replace with states for id2 and id3 only
    let new_states = vec![make_health_state(&id2, 2), make_health_state(&id3, 1)];
    replace_record_health_states(&db, &new_states).unwrap();

    let states = list_record_health_states(&db).unwrap();
    assert_eq!(states.len(), 2);

    // id1 should be gone
    assert!(get_record_health_state(&db, &id1).unwrap().is_none());
    // id2 should have version 2
    let s2 = get_record_health_state(&db, &id2).unwrap().unwrap();
    assert_eq!(s2.record_version, 2);
    // id3 should exist
    assert!(get_record_health_state(&db, &id3).unwrap().is_some());
}

#[test]
fn replace_record_health_states_with_empty_clears_all() {
    let db = fresh_db();
    let id = Uuid::new_v4();
    insert_record(&db, &make_test_record(&id, 1)).unwrap();
    upsert_record_health_state(&db, &make_health_state(&id, 1)).unwrap();
    assert_eq!(list_record_health_states(&db).unwrap().len(), 1);

    replace_record_health_states(&db, &[]).unwrap();
    assert!(list_record_health_states(&db).unwrap().is_empty());
}

#[test]
fn delete_record_health_state_removes_single() {
    let db = fresh_db();
    let id = Uuid::new_v4();
    insert_record(&db, &make_test_record(&id, 1)).unwrap();
    upsert_record_health_state(&db, &make_health_state(&id, 1)).unwrap();

    let result = delete_record_health_state(&db, &id).unwrap();
    assert!(result, "should return true when row exists");

    assert!(get_record_health_state(&db, &id).unwrap().is_none());
}

#[test]
fn delete_record_health_state_returns_false_when_not_found() {
    let db = fresh_db();
    let id = Uuid::new_v4();
    let result = delete_record_health_state(&db, &id).unwrap();
    assert!(!result, "should return false when no row exists");
}

#[test]
fn delete_record_health_states_batch() {
    let db = fresh_db();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    insert_record(&db, &make_test_record(&id1, 1)).unwrap();
    insert_record(&db, &make_test_record(&id2, 1)).unwrap();
    insert_record(&db, &make_test_record(&id3, 1)).unwrap();

    upsert_record_health_state(&db, &make_health_state(&id1, 1)).unwrap();
    upsert_record_health_state(&db, &make_health_state(&id2, 1)).unwrap();
    upsert_record_health_state(&db, &make_health_state(&id3, 1)).unwrap();

    let affected = delete_record_health_states(&db, &[id1, id3]).unwrap();
    assert_eq!(affected, 2);

    assert!(get_record_health_state(&db, &id1).unwrap().is_none());
    assert!(get_record_health_state(&db, &id2).unwrap().is_some());
    assert!(get_record_health_state(&db, &id3).unwrap().is_none());
}

#[test]
fn delete_record_health_states_handles_empty_ids() {
    let db = fresh_db();
    let affected = delete_record_health_states(&db, &[]).unwrap();
    assert_eq!(affected, 0);
}

#[test]
fn copy_record_health_state_version_advances_version() {
    let db = fresh_db();
    let id = Uuid::new_v4();
    insert_record(&db, &make_test_record(&id, 1)).unwrap();
    upsert_record_health_state(&db, &make_health_state(&id, 1)).unwrap();

    let result = copy_record_health_state_version(&db, &id, 5).unwrap();
    assert!(result, "should return true when row exists");

    let fetched = get_record_health_state(&db, &id).unwrap().unwrap();
    assert_eq!(fetched.record_version, 5);
    // Other fields should be preserved
    assert_eq!(fetched.weak_password, Some(true));
    assert_eq!(fetched.duplicate_group_size, Some(3));
}

#[test]
fn copy_record_health_state_version_returns_false_when_not_found() {
    let db = fresh_db();
    let id = Uuid::new_v4();
    let result = copy_record_health_state_version(&db, &id, 2).unwrap();
    assert!(!result, "should return false when no row exists");
}

#[test]
fn health_state_null_fields_round_trip() {
    let db = fresh_db();
    let id = Uuid::new_v4();
    insert_record(&db, &make_test_record(&id, 1)).unwrap();

    // All tri-state fields are None — "not yet evaluated"
    let state = RecordHealthState {
        record_id: id,
        record_version: 1,
        evaluated_at: None,
        weak_password: None,
        duplicate_group_size: None,
        compromised: None,
        expired: None,
    };
    upsert_record_health_state(&db, &state).unwrap();

    let fetched = get_record_health_state(&db, &id).unwrap().unwrap();
    assert_eq!(fetched.record_id, id);
    assert_eq!(fetched.record_version, 1);
    assert!(fetched.evaluated_at.is_none());
    assert!(fetched.weak_password.is_none());
    assert!(fetched.duplicate_group_size.is_none());
    assert!(fetched.compromised.is_none());
    assert!(fetched.expired.is_none());
}
