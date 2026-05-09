use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use super::*;
use crate::db::models::datetime_to_timestamp;
use crate::db::schema;
use crate::types::audit::AuditOperation;
use crate::types::credential::CredentialType;
use crate::types::record::StoredRecord;

/// Create an in-memory database with schema initialized.
fn setup_db() -> Connection {
    schema::init_db_in_memory()
}

/// Build a test `StoredRecord` with sensible defaults.
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

// -----------------------------------------------------------------------
// 1. update_record
// -----------------------------------------------------------------------

#[test]
fn update_record_succeeds_when_version_matches() {
    let conn = setup_db();
    let id = Uuid::new_v4();
    let record = make_test_record(&id, 1);
    insert_record(&conn, &record).unwrap();

    let mut updated = record.clone();
    updated.encrypted_data = vec![9, 8, 7];
    updated.updated_by = "updater".to_string();
    updated.version = 2;

    let result = update_record(&conn, &updated, 1).unwrap();
    assert!(result, "should return true when version matches");

    let fetched = get_record(&conn, &id).unwrap().unwrap();
    assert_eq!(fetched.encrypted_data, vec![9, 8, 7]);
    assert_eq!(fetched.updated_by, "updater");
    assert_eq!(fetched.version, 2);
}

#[test]
fn update_record_fails_when_version_mismatch() {
    let conn = setup_db();
    let id = Uuid::new_v4();
    let record = make_test_record(&id, 1);
    insert_record(&conn, &record).unwrap();

    let mut updated = record.clone();
    updated.version = 2;

    let result = update_record(&conn, &updated, 99).unwrap();
    assert!(!result, "should return false when version does not match");
}

#[test]
fn update_record_fails_when_record_not_found() {
    let conn = setup_db();
    let id = Uuid::new_v4();
    let record = make_test_record(&id, 1);
    // Not inserted

    let result = update_record(&conn, &record, 1).unwrap();
    assert!(!result, "should return false when record does not exist");
}

// -----------------------------------------------------------------------
// 2. list_deleted_records
// -----------------------------------------------------------------------

#[test]
fn list_deleted_records_returns_only_soft_deleted() {
    let conn = setup_db();
    let id_active = Uuid::new_v4();
    let id_deleted = Uuid::new_v4();

    insert_record(&conn, &make_test_record(&id_active, 1)).unwrap();
    insert_record(&conn, &make_test_record(&id_deleted, 1)).unwrap();
    soft_delete_record(&conn, &id_deleted).unwrap();

    let deleted = list_deleted_records(&conn).unwrap();
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].id, id_deleted);
    assert!(deleted[0].deleted);
}

#[test]
fn list_deleted_records_returns_empty_when_none_deleted() {
    let conn = setup_db();
    let deleted = list_deleted_records(&conn).unwrap();
    assert!(deleted.is_empty());
}

// -----------------------------------------------------------------------
// 3. insert_password_history
// -----------------------------------------------------------------------

#[test]
fn insert_password_history_adds_entry() {
    let conn = setup_db();
    let id = Uuid::new_v4();
    insert_record(&conn, &make_test_record(&id, 1)).unwrap();

    let nonce = [42u8; 24];
    let now_ts = datetime_to_timestamp(&Utc::now());
    insert_password_history(&conn, &id, &[1, 2, 3], &nonce, 1, now_ts).unwrap();

    let count = count_password_history(&conn, &id).unwrap();
    assert_eq!(count, 1);
}

// -----------------------------------------------------------------------
// 4. get_password_history
// -----------------------------------------------------------------------

#[test]
fn get_password_history_returns_entries_ordered_desc() {
    let conn = setup_db();
    let id = Uuid::new_v4();
    insert_record(&conn, &make_test_record(&id, 1)).unwrap();

    let nonce = [0u8; 24];
    insert_password_history(&conn, &id, &[1], &nonce, 1, 100).unwrap();
    insert_password_history(&conn, &id, &[2], &nonce, 1, 200).unwrap();
    insert_password_history(&conn, &id, &[3], &nonce, 1, 300).unwrap();

    let history = get_password_history(&conn, &id, 10).unwrap();
    assert_eq!(history.len(), 3);
    // Descending order by changed_at
    assert_eq!(history[0].encrypted_password, vec![3]);
    assert_eq!(history[1].encrypted_password, vec![2]);
    assert_eq!(history[2].encrypted_password, vec![1]);
}

#[test]
fn get_password_history_respects_limit() {
    let conn = setup_db();
    let id = Uuid::new_v4();
    insert_record(&conn, &make_test_record(&id, 1)).unwrap();

    let nonce = [0u8; 24];
    insert_password_history(&conn, &id, &[1], &nonce, 1, 100).unwrap();
    insert_password_history(&conn, &id, &[2], &nonce, 1, 200).unwrap();

    let history = get_password_history(&conn, &id, 1).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].encrypted_password, vec![2]);
}

// -----------------------------------------------------------------------
// 5. count_password_history
// -----------------------------------------------------------------------

#[test]
fn count_password_history_returns_zero_for_no_entries() {
    let conn = setup_db();
    let id = Uuid::new_v4();
    assert_eq!(count_password_history(&conn, &id).unwrap(), 0);
}

// -----------------------------------------------------------------------
// 6. delete_oldest_password_history
// -----------------------------------------------------------------------

#[test]
fn delete_oldest_password_history_removes_earliest() {
    let conn = setup_db();
    let id = Uuid::new_v4();
    insert_record(&conn, &make_test_record(&id, 1)).unwrap();

    let nonce = [0u8; 24];
    insert_password_history(&conn, &id, &[1], &nonce, 1, 100).unwrap();
    insert_password_history(&conn, &id, &[2], &nonce, 1, 200).unwrap();

    delete_oldest_password_history(&conn, &id).unwrap();

    let history = get_password_history(&conn, &id, 10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].encrypted_password, vec![2]);
}

// -----------------------------------------------------------------------
// 7. delete_password_history_by_record
// -----------------------------------------------------------------------

#[test]
fn delete_password_history_by_record_removes_all() {
    let conn = setup_db();
    let id = Uuid::new_v4();
    insert_record(&conn, &make_test_record(&id, 1)).unwrap();

    let nonce = [0u8; 24];
    insert_password_history(&conn, &id, &[1], &nonce, 1, 100).unwrap();
    insert_password_history(&conn, &id, &[2], &nonce, 1, 200).unwrap();
    insert_password_history(&conn, &id, &[3], &nonce, 1, 300).unwrap();

    delete_password_history_by_record(&conn, &id).unwrap();

    assert_eq!(count_password_history(&conn, &id).unwrap(), 0);
}

// -----------------------------------------------------------------------
// 8. batch_soft_delete_records
// -----------------------------------------------------------------------

#[test]
fn batch_soft_delete_records_deletes_multiple() {
    let conn = setup_db();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    insert_record(&conn, &make_test_record(&id1, 1)).unwrap();
    insert_record(&conn, &make_test_record(&id2, 1)).unwrap();
    insert_record(&conn, &make_test_record(&id3, 1)).unwrap();

    let affected = batch_soft_delete_records(&conn, &[id1, id2], "admin").unwrap();
    assert_eq!(affected, 2);

    let deleted = list_deleted_records(&conn).unwrap();
    assert_eq!(deleted.len(), 2);

    // id3 should still be active
    let active = list_active_records(&conn).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, id3);
}

#[test]
fn batch_soft_delete_records_handles_empty_ids() {
    let conn = setup_db();
    let affected = batch_soft_delete_records(&conn, &[], "admin").unwrap();
    assert_eq!(affected, 0);
}

#[test]
fn batch_soft_delete_records_updates_deleted_by() {
    let conn = setup_db();
    let id = Uuid::new_v4();
    insert_record(&conn, &make_test_record(&id, 1)).unwrap();

    batch_soft_delete_records(&conn, &[id], "deleter_user").unwrap();

    let deleted = list_deleted_records(&conn).unwrap();
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].updated_by, "deleter_user");
}

// -----------------------------------------------------------------------
// 9. list_audit_entries_filtered
// -----------------------------------------------------------------------

#[test]
fn list_audit_entries_filtered_with_no_filters() {
    let conn = setup_db();
    insert_audit_entry(
        &conn,
        AuditOperation::RecordCreate,
        None,
        Some("test"),
        None,
    )
    .unwrap();

    let entries = list_audit_entries_filtered(&conn, None, None, None, None, 10, 0).unwrap();
    assert_eq!(entries.len(), 1);
}

#[test]
fn list_audit_entries_filtered_by_operation() {
    let conn = setup_db();
    insert_audit_entry(&conn, AuditOperation::RecordCreate, None, None, None).unwrap();
    insert_audit_entry(&conn, AuditOperation::RecordDelete, None, None, None).unwrap();

    let entries =
        list_audit_entries_filtered(&conn, Some("record.create"), None, None, None, 10, 0).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].operation, AuditOperation::RecordCreate);
}

#[test]
fn list_audit_entries_filtered_by_time_range() {
    let conn = setup_db();
    // Insert entries with specific timestamps by inserting directly
    conn.execute(
        "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["record.create", None::<String>, "early", None::<String>, 1000],
    ).unwrap();
    conn.execute(
        "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["record.update", None::<String>, "middle", None::<String>, 2000],
    ).unwrap();
    conn.execute(
        "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["record.delete", None::<String>, "late", None::<String>, 3000],
    ).unwrap();

    let entries =
        list_audit_entries_filtered(&conn, None, Some(1500), Some(2500), None, 10, 0).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].record_name.as_deref(), Some("middle"));
}

#[test]
fn list_audit_entries_filtered_by_search() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["record.create", None::<String>, "GitHub Token", None::<String>, 1000],
    ).unwrap();
    conn.execute(
        "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["record.create", None::<String>, "AWS Key", "some detail", 2000],
    ).unwrap();

    let entries =
        list_audit_entries_filtered(&conn, None, None, None, Some("GitHub"), 10, 0).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].record_name.as_deref(), Some("GitHub Token"));
}

#[test]
fn list_audit_entries_filtered_respects_pagination() {
    let conn = setup_db();
    for i in 0..5 {
        conn.execute(
            "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["record.create", None::<String>, format!("entry_{i}"), None::<String>, i * 100],
        ).unwrap();
    }

    let page = list_audit_entries_filtered(&conn, None, None, None, None, 2, 1).unwrap();
    assert_eq!(page.len(), 2);
}

// -----------------------------------------------------------------------
// 10. cleanup_audit_entries
// -----------------------------------------------------------------------

#[test]
fn cleanup_audit_entries_removes_old_entries() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
        rusqlite::params!["record.create", 100],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
        rusqlite::params!["record.create", 200],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
        rusqlite::params!["record.create", 300],
    )
    .unwrap();

    let deleted = cleanup_audit_entries(&conn, 250).unwrap();
    assert_eq!(deleted, 2);

    let remaining = list_audit_entries(&conn, 100, 0).unwrap();
    assert_eq!(remaining.len(), 1);
}

#[test]
fn cleanup_audit_entries_removes_none_when_all_newer() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
        rusqlite::params!["record.create", 5000],
    )
    .unwrap();

    let deleted = cleanup_audit_entries(&conn, 100).unwrap();
    assert_eq!(deleted, 0);
}

// -----------------------------------------------------------------------
// 11. count_audit_entries
// -----------------------------------------------------------------------

#[test]
fn count_audit_entries_with_no_filters() {
    let conn = setup_db();
    insert_audit_entry(&conn, AuditOperation::RecordCreate, None, None, None).unwrap();
    insert_audit_entry(&conn, AuditOperation::RecordDelete, None, None, None).unwrap();

    let count = count_audit_entries(&conn, None, None, None, None).unwrap();
    assert_eq!(count, 2);
}

#[test]
fn count_audit_entries_with_operation_filter() {
    let conn = setup_db();
    insert_audit_entry(&conn, AuditOperation::RecordCreate, None, None, None).unwrap();
    insert_audit_entry(&conn, AuditOperation::RecordDelete, None, None, None).unwrap();

    let count = count_audit_entries(&conn, Some("record.create"), None, None, None).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn count_audit_entries_with_search_filter() {
    let conn = setup_db();
    conn.execute(
        "INSERT INTO audit_log (operation, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["record.create", "GitHub", "created new", 1000],
    ).unwrap();
    conn.execute(
        "INSERT INTO audit_log (operation, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["record.create", "AWS", "created aws", 2000],
    ).unwrap();

    let count = count_audit_entries(&conn, None, None, None, Some("Git")).unwrap();
    assert_eq!(count, 1);
}

// -----------------------------------------------------------------------
// 12. rename_tag
// -----------------------------------------------------------------------

#[test]
fn rename_tag_succeeds_when_tag_exists() {
    let conn = setup_db();
    insert_tag(&conn, "old-name").unwrap();

    let result = rename_tag(&conn, "old-name", "new-name").unwrap();
    assert!(result);

    let tag = get_tag_by_name(&conn, "new-name").unwrap().unwrap();
    assert_eq!(tag.name, "new-name");
}

#[test]
fn rename_tag_returns_false_when_not_found() {
    let conn = setup_db();
    let result = rename_tag(&conn, "nonexistent", "something").unwrap();
    assert!(!result);
}

// -----------------------------------------------------------------------
// 13. delete_tag_by_name
// -----------------------------------------------------------------------

#[test]
fn delete_tag_by_name_removes_tag() {
    let conn = setup_db();
    insert_tag(&conn, "to-delete").unwrap();

    let result = delete_tag_by_name(&conn, "to-delete").unwrap();
    assert!(result);

    let tag = get_tag_by_name(&conn, "to-delete").unwrap();
    assert!(tag.is_none());
}

#[test]
fn delete_tag_by_name_returns_false_when_not_found() {
    let conn = setup_db();
    let result = delete_tag_by_name(&conn, "nonexistent").unwrap();
    assert!(!result);
}

// -----------------------------------------------------------------------
// 14. get_tag_by_name
// -----------------------------------------------------------------------

#[test]
fn get_tag_by_name_returns_tag_when_found() {
    let conn = setup_db();
    let created = insert_tag(&conn, "my-tag").unwrap();

    let found = get_tag_by_name(&conn, "my-tag").unwrap().unwrap();
    assert_eq!(found.id, created.id);
    assert_eq!(found.name, "my-tag");
}

#[test]
fn get_tag_by_name_returns_none_when_not_found() {
    let conn = setup_db();
    let found = get_tag_by_name(&conn, "no-such-tag").unwrap();
    assert!(found.is_none());
}

// -----------------------------------------------------------------------
// 15. delete_metadata
// -----------------------------------------------------------------------

#[test]
fn delete_metadata_removes_existing_key() {
    let conn = setup_db();
    set_metadata(&conn, "test_key", "test_value").unwrap();

    let result = delete_metadata(&conn, "test_key").unwrap();
    assert!(result);

    let value = get_metadata(&conn, "test_key").unwrap();
    assert!(value.is_none());
}

#[test]
fn delete_metadata_returns_false_for_missing_key() {
    let conn = setup_db();
    let result = delete_metadata(&conn, "nonexistent_key").unwrap();
    assert!(!result);
}
