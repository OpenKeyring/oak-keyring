//! Integration tests for the full record lifecycle.
//!
//! Covers: create -> tag -> soft delete -> restore -> hard delete,
//! and coexistence of multiple credential types.

use chrono::Utc;
use uuid::Uuid;

use oak_keyring::db::init_db_in_memory;
use oak_keyring::db::queries;
use oak_keyring::types::credential::CredentialType;
use oak_keyring::types::record::StoredRecord;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_record(id: &str, ct: CredentialType) -> StoredRecord {
    StoredRecord {
        id: Uuid::parse_str(id).unwrap(),
        credential_type: ct,
        encrypted_data: vec![1, 2, 3],
        nonce: [7u8; 24],
        dek_version: 1,
        aad: vec![],
        is_favorite: false,
        expires_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        updated_by: "device-test".to_string(),
        version: 1,
        deleted: false,
        deleted_at: None,
        tags: vec![],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn full_record_lifecycle() {
    let conn = init_db_in_memory();
    let id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();

    // -- Create a Login record --
    let rec = make_record(
        "11111111-1111-1111-1111-111111111111",
        CredentialType::Login,
    );
    queries::insert_record(&conn, &rec).unwrap();

    let fetched = queries::get_record(&conn, &id)
        .unwrap()
        .expect("record should exist");
    assert_eq!(fetched.id, id);
    assert_eq!(fetched.credential_type, CredentialType::Login);
    assert!(!fetched.deleted);

    // -- Add a tag and attach to the record --
    let tag = queries::insert_tag(&conn, "work").unwrap();
    assert_eq!(tag.name, "work");

    queries::attach_tag(&conn, &id, tag.id).unwrap();
    let tags = queries::get_record_tags(&conn, &id).unwrap();
    assert_eq!(tags, vec!["work"]);

    // Verify the record appears in the active list.
    let active = queries::list_active_records(&conn).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, id);

    // -- Soft delete --
    queries::soft_delete_record(&conn, &id).unwrap();

    // Record disappears from active list.
    let active = queries::list_active_records(&conn).unwrap();
    assert!(
        active.is_empty(),
        "soft-deleted record should not appear in active list"
    );

    // Record is still fetchable by ID and marked deleted.
    let fetched = queries::get_record(&conn, &id)
        .unwrap()
        .expect("record should still exist");
    assert!(fetched.deleted);
    assert!(fetched.deleted_at.is_some());

    // Tags are preserved after soft delete.
    let tags = queries::get_record_tags(&conn, &id).unwrap();
    assert_eq!(tags, vec!["work"], "tags should survive soft delete");

    // -- Restore --
    queries::restore_record(&conn, &id).unwrap();

    // Record reappears in active list.
    let active = queries::list_active_records(&conn).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, id);
    assert!(!active[0].deleted);

    // -- Hard delete --
    queries::hard_delete_record(&conn, &id).unwrap();

    // Record is completely gone.
    let fetched = queries::get_record(&conn, &id).unwrap();
    assert!(fetched.is_none(), "hard-deleted record should return None");

    // record_tags cascade removed: no tags linked to this record.
    let tags = queries::get_record_tags(&conn, &id).unwrap();
    assert!(tags.is_empty(), "record_tags should cascade on hard delete");

    // Tag definition survives.
    let all_tags = queries::list_tags(&conn).unwrap();
    assert_eq!(
        all_tags.len(),
        1,
        "tag definition should survive hard delete"
    );
    assert_eq!(all_tags[0].name, "work");
}

#[test]
fn multiple_record_types_coexist() {
    let conn = init_db_in_memory();

    let recs = [
        make_record(
            "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            CredentialType::Login,
        ),
        make_record("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", CredentialType::Api),
        make_record("cccccccc-cccc-cccc-cccc-cccccccccccc", CredentialType::Ssh),
    ];

    for rec in &recs {
        queries::insert_record(&conn, rec).unwrap();
    }

    let active = queries::list_active_records(&conn).unwrap();
    assert_eq!(active.len(), 3, "should list exactly 3 active records");

    let types: std::collections::HashSet<CredentialType> =
        active.iter().map(|r| r.credential_type).collect();

    assert!(
        types.contains(&CredentialType::Login),
        "should contain Login type"
    );
    assert!(
        types.contains(&CredentialType::Api),
        "should contain Api type"
    );
    assert!(
        types.contains(&CredentialType::Ssh),
        "should contain Ssh type"
    );
}
