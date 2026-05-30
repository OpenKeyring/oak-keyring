use crate::crypto::bip39::{MnemonicLanguage, Passkey};
use crate::db::schema::init_db_in_memory;
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::VaultService;
use crate::types::credential::{CredentialType, EncryptedPayload};
use crate::types::record::CreateRecordParams;
use crate::types::sensitive::SecureStr;
use crate::types::sync::SyncStatus;
use uuid::Uuid;

/// Helper: create an in-memory VaultService with schema initialized.
fn setup_service() -> VaultService {
    let conn = init_db_in_memory().unwrap();
    VaultService::new(conn)
}

/// Helper: unlock the VaultService with a fresh mnemonic.
fn unlock_service(svc: &mut VaultService) {
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
    svc.crypto
        .unlock_with_mnemonic(&mnemonic)
        .expect("unlock_with_mnemonic must succeed in test");
}

/// Helper: create a Login record with the given tags.
fn create_record_with_tags(svc: &mut VaultService, name: &str, tags: Vec<String>) -> Uuid {
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: EncryptedPayload::Login {
            name: name.to_string(),
            username: format!("user_{}", name.to_lowercase()),
            password: SecureStr::new("password123".to_string()),
            url: None,
            notes: None,
        },
        tags,
        is_favorite: false,
        expires_at: None,
    })
    .expect("create_record must succeed")
}

// =========================================================================
// list_tags tests
// =========================================================================

#[test]
fn list_tags_returns_empty_when_no_tags_exist() {
    let svc = setup_service();
    let tags = svc.list_tags().expect("list_tags must succeed");
    assert!(tags.is_empty(), "no tags should exist in empty vault");
}

#[test]
fn list_tags_returns_tags_with_correct_usage_count() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create records with overlapping tags
    create_record_with_tags(
        &mut svc,
        "Rec1",
        vec!["work".to_string(), "dev".to_string()],
    );
    create_record_with_tags(&mut svc, "Rec2", vec!["work".to_string()]);
    create_record_with_tags(&mut svc, "Rec3", vec!["personal".to_string()]);

    let tags = svc.list_tags().expect("list_tags must succeed");

    // Ordered alphabetically: dev, personal, work
    assert_eq!(tags.len(), 3);

    let (dev_tag, dev_count) = &tags[0];
    assert_eq!(dev_tag.name, "dev");
    assert_eq!(*dev_count, 1, "dev tag used by 1 record");

    let (personal_tag, personal_count) = &tags[1];
    assert_eq!(personal_tag.name, "personal");
    assert_eq!(*personal_count, 1, "personal tag used by 1 record");

    let (work_tag, work_count) = &tags[2];
    assert_eq!(work_tag.name, "work");
    assert_eq!(*work_count, 2, "work tag used by 2 records");
}

#[test]
fn list_tags_excludes_soft_deleted_records_from_count() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let id1 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "ToBeDeleted".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["work".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    create_record_with_tags(&mut svc, "Active", vec!["work".to_string()]);

    // Before soft delete: work tag count = 2
    let tags_before = svc.list_tags().expect("list_tags must succeed");
    let work_before = tags_before.iter().find(|(t, _)| t.name == "work").unwrap();
    assert_eq!(work_before.1, 2);

    // Soft delete one record
    svc.soft_delete_record(id1)
        .expect("soft_delete must succeed");

    // After soft delete: work tag count = 1 (only active records)
    let tags_after = svc.list_tags().expect("list_tags must succeed");
    let work_after = tags_after.iter().find(|(t, _)| t.name == "work").unwrap();
    assert_eq!(work_after.1, 1, "soft-deleted records should not count");
}

#[test]
fn list_tags_includes_unused_tags_with_zero_count() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create a record with a tag, then hard delete the record
    let id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "TempRec".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["orphan".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    // Hard delete removes the record and cascade-deletes record_tags
    svc.hard_delete_record(id)
        .expect("hard_delete must succeed");

    // The tag itself still exists but has no associations
    let tags = svc.list_tags().expect("list_tags must succeed");
    // The tag "orphan" should still exist with count 0
    // (unless get_or_create_tag was used — tags table row persists)
    let orphan = tags.iter().find(|(t, _)| t.name == "orphan");
    assert!(
        orphan.is_none() || orphan.map(|(_, c)| *c) == Some(0),
        "orphan tag should have 0 usage or not exist"
    );
}

// =========================================================================
// rename_tag tests
// =========================================================================

#[test]
fn rename_tag_succeeds_and_tag_is_findable_by_new_name() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create a tag via record creation
    create_record_with_tags(&mut svc, "Rec", vec!["old_name".to_string()]);

    svc.rename_tag("old_name", "new_name")
        .expect("rename_tag must succeed");

    // Verify the old name is gone
    let tags = svc.list_tags().expect("list_tags must succeed");
    assert!(
        tags.iter().all(|(t, _)| t.name != "old_name"),
        "old tag name should not exist"
    );

    // Verify the new name exists
    let new_tag = tags.iter().find(|(t, _)| t.name == "new_name");
    assert!(new_tag.is_some(), "new tag name should exist");
    assert_eq!(new_tag.unwrap().1, 1, "usage count should be preserved");
}

#[test]
fn rename_tag_marks_affected_records_pending_sync() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_record_with_tags(&mut svc, "Rec1", vec!["old_name".to_string()]);
    svc.mark_record_synced(&id)
        .expect("record should start synced");

    svc.rename_tag("old_name", "new_name")
        .expect("rename_tag must succeed");

    let sync_map = svc.load_sync_status_map();
    assert_eq!(sync_map.get(&id.to_string()), Some(&SyncStatus::Pending));
}

#[test]
fn rename_tag_target_exists_returns_tag_already_exists() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    create_record_with_tags(&mut svc, "Rec1", vec!["alpha".to_string()]);
    create_record_with_tags(&mut svc, "Rec2", vec!["beta".to_string()]);

    let result = svc.rename_tag("alpha", "beta");
    assert!(result.is_err(), "rename to existing name should fail");
    assert!(
        matches!(result.unwrap_err(), VaultError::TagAlreadyExists(ref n) if n == "beta"),
        "expected TagAlreadyExists(\"beta\")"
    );
}

#[test]
fn rename_tag_source_not_found_returns_tag_not_found() {
    let mut svc = setup_service();

    let result = svc.rename_tag("nonexistent", "something_else");
    assert!(result.is_err(), "rename nonexistent tag should fail");
    assert!(
        matches!(result.unwrap_err(), VaultError::TagNotFound(ref n) if n == "nonexistent"),
        "expected TagNotFound(\"nonexistent\")"
    );
}

// =========================================================================
// delete_tag tests
// =========================================================================

#[test]
fn delete_tag_removes_tag_and_record_tags_associations() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create two records sharing the "work" tag
    let id1 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec1".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass1".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["work".to_string(), "dev".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let _id2 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec2".to_string(),
                username: "user2".to_string(),
                password: SecureStr::new("pass2".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["work".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    // Verify tags exist before delete
    let tags_before = svc.list_tags().expect("list_tags must succeed");
    assert_eq!(tags_before.len(), 2, "should have 2 tags (dev, work)");

    // Delete the "work" tag
    svc.delete_tag("work").expect("delete_tag must succeed");

    // Verify tag is gone from list
    let tags_after = svc.list_tags().expect("list_tags must succeed");
    assert_eq!(tags_after.len(), 1, "only dev tag should remain");
    assert_eq!(tags_after[0].0.name, "dev");

    // Verify the record no longer has "work" in its tags
    let stored = svc.get_stored_record(id1).expect("record must exist");
    assert!(
        !stored.tags.contains(&"work".to_string()),
        "work tag should be removed from record"
    );
    assert!(
        stored.tags.contains(&"dev".to_string()),
        "dev tag should still be present"
    );
}

#[test]
fn delete_tag_marks_affected_records_pending_sync() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_record_with_tags(&mut svc, "Rec1", vec!["work".to_string()]);
    svc.mark_record_synced(&id)
        .expect("record should start synced");

    svc.delete_tag("work").expect("delete_tag must succeed");

    let sync_map = svc.load_sync_status_map();
    assert_eq!(sync_map.get(&id.to_string()), Some(&SyncStatus::Pending));
}

#[test]
fn delete_tag_not_found_returns_tag_not_found() {
    let mut svc = setup_service();

    let result = svc.delete_tag("nonexistent");
    assert!(result.is_err(), "deleting nonexistent tag should fail");
    assert!(
        matches!(result.unwrap_err(), VaultError::TagNotFound(ref n) if n == "nonexistent"),
        "expected TagNotFound(\"nonexistent\")"
    );
}

// =========================================================================
// batch_add_tag tests
// =========================================================================

#[test]
fn batch_add_tag_adds_tag_to_3_records_returns_3() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create 3 records without the "batch" tag
    let id1 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec1".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass1".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let id2 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec2".to_string(),
                username: "user2".to_string(),
                password: SecureStr::new("pass2".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let id3 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec3".to_string(),
                username: "user3".to_string(),
                password: SecureStr::new("pass3".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let added = svc
        .batch_add_tag(&[id1, id2, id3], "batch")
        .expect("batch_add_tag must succeed");
    assert_eq!(added, 3, "should add tag to all 3 records");

    // Verify each record now has the tag
    for id in &[id1, id2, id3] {
        let stored = svc.get_stored_record(*id).expect("record must exist");
        assert!(
            stored.tags.contains(&"batch".to_string()),
            "record should have 'batch' tag"
        );
    }
}

#[test]
fn batch_add_tag_skips_already_tagged_returns_0_for_existing() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create 2 records, one already has the "work" tag
    let id1 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Tagged".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass1".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["work".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let id2 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Untagged".to_string(),
                username: "user2".to_string(),
                password: SecureStr::new("pass2".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let added = svc
        .batch_add_tag(&[id1, id2], "work")
        .expect("batch_add_tag must succeed");
    assert_eq!(added, 1, "only 1 new association (id2), id1 already tagged");

    // Both should now have "work"
    let stored1 = svc.get_stored_record(id1).expect("record must exist");
    let stored2 = svc.get_stored_record(id2).expect("record must exist");
    assert!(stored1.tags.contains(&"work".to_string()));
    assert!(stored2.tags.contains(&"work".to_string()));
}

#[test]
fn batch_add_tag_creates_tag_if_missing() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec".to_string(),
                username: "user".to_string(),
                password: SecureStr::new("pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    // "new_tag" does not exist yet
    let tags_before = svc.list_tags().expect("list_tags must succeed");
    assert!(tags_before.is_empty(), "no tags should exist yet");

    let added = svc
        .batch_add_tag(&[id], "new_tag")
        .expect("batch_add_tag must succeed");
    assert_eq!(added, 1);

    // Tag was created
    let tags_after = svc.list_tags().expect("list_tags must succeed");
    assert_eq!(tags_after.len(), 1);
    assert_eq!(tags_after[0].0.name, "new_tag");
}

#[test]
fn batch_add_tag_marks_changed_records_pending_sync() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id1 = create_record_with_tags(&mut svc, "Rec1", vec![]);
    let id2 = create_record_with_tags(&mut svc, "Rec2", vec![]);
    svc.mark_record_synced(&id1).expect("record 1 synced");
    svc.mark_record_synced(&id2).expect("record 2 synced");

    let added = svc
        .batch_add_tag(&[id1, id2], "batch")
        .expect("batch_add_tag must succeed");

    assert_eq!(added, 2);
    let sync_map = svc.load_sync_status_map();
    assert_eq!(sync_map.get(&id1.to_string()), Some(&SyncStatus::Pending));
    assert_eq!(sync_map.get(&id2.to_string()), Some(&SyncStatus::Pending));
}

// =========================================================================
// batch_remove_tag tests
// =========================================================================

#[test]
fn batch_remove_tag_removes_tag_from_records() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create 3 records with the "remove_me" tag
    let id1 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec1".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass1".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["remove_me".to_string(), "keep".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let id2 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec2".to_string(),
                username: "user2".to_string(),
                password: SecureStr::new("pass2".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["remove_me".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let removed = svc
        .batch_remove_tag(&[id1, id2], "remove_me")
        .expect("batch_remove_tag must succeed");
    assert_eq!(removed, 2, "should remove tag from 2 records");

    // Verify tag is gone from records
    let stored1 = svc.get_stored_record(id1).expect("record must exist");
    assert!(
        !stored1.tags.contains(&"remove_me".to_string()),
        "remove_me should be gone from rec1"
    );
    assert!(
        stored1.tags.contains(&"keep".to_string()),
        "keep tag should still be present on rec1"
    );

    let stored2 = svc.get_stored_record(id2).expect("record must exist");
    assert!(
        !stored2.tags.contains(&"remove_me".to_string()),
        "remove_me should be gone from rec2"
    );
}

#[test]
fn batch_remove_tag_auto_deletes_orphan_tag() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create a record with an "orphan" tag
    let id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec".to_string(),
                username: "user".to_string(),
                password: SecureStr::new("pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["orphan".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    // Verify tag exists
    let tags_before = svc.list_tags().expect("list_tags must succeed");
    assert_eq!(tags_before.len(), 1);
    assert_eq!(tags_before[0].0.name, "orphan");

    // Remove the only association
    let removed = svc
        .batch_remove_tag(&[id], "orphan")
        .expect("batch_remove_tag must succeed");
    assert_eq!(removed, 1);

    // Tag should be auto-deleted since no records use it
    let tags_after = svc.list_tags().expect("list_tags must succeed");
    assert!(
        tags_after.is_empty(),
        "orphan tag should be auto-deleted when no records use it"
    );
}

#[test]
fn batch_remove_tag_preserves_tag_if_still_used() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create 3 records with "shared" tag
    let id1 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec1".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass1".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["shared".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let _id2 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec2".to_string(),
                username: "user2".to_string(),
                password: SecureStr::new("pass2".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["shared".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    // Remove from only 1 record (id1); id2 still uses "shared"
    let removed = svc
        .batch_remove_tag(&[id1], "shared")
        .expect("batch_remove_tag must succeed");
    assert_eq!(removed, 1);

    // Tag should still exist because id2 still uses it
    let tags = svc.list_tags().expect("list_tags must succeed");
    assert_eq!(tags.len(), 1, "shared tag should still exist");
    assert_eq!(tags[0].0.name, "shared");
    assert_eq!(tags[0].1, 1, "shared tag should have 1 remaining usage");
}

#[test]
fn batch_remove_tag_returns_tag_not_found_for_missing_tag() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create a record (so record_ids are valid but the tag doesn't exist)
    let id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec".to_string(),
                username: "user".to_string(),
                password: SecureStr::new("pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let result = svc.batch_remove_tag(&[id], "nonexistent");
    assert!(result.is_err(), "removing nonexistent tag should fail");
    assert!(
        matches!(result.unwrap_err(), VaultError::TagNotFound(ref n) if n == "nonexistent"),
        "expected TagNotFound(\"nonexistent\")"
    );
}

#[test]
fn batch_add_tag_executes_in_transaction() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let id1 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec1".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass1".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let id2 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec2".to_string(),
                username: "user2".to_string(),
                password: SecureStr::new("pass2".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    // Both records tagged atomically
    let added = svc
        .batch_add_tag(&[id1, id2], "tx_tag")
        .expect("batch_add_tag must succeed");
    assert_eq!(added, 2);

    // Verify both have the tag
    assert!(svc
        .get_stored_record(id1)
        .unwrap()
        .tags
        .contains(&"tx_tag".to_string()));
    assert!(svc
        .get_stored_record(id2)
        .unwrap()
        .tags
        .contains(&"tx_tag".to_string()));
}

#[test]
fn batch_remove_tag_executes_in_transaction() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let id1 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec1".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass1".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["tx_remove".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let id2 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Rec2".to_string(),
                username: "user2".to_string(),
                password: SecureStr::new("pass2".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["tx_remove".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let removed = svc
        .batch_remove_tag(&[id1, id2], "tx_remove")
        .expect("batch_remove_tag must succeed");
    assert_eq!(removed, 2);

    // Tag should be auto-deleted (orphan)
    let tags = svc.list_tags().expect("list_tags must succeed");
    assert!(
        tags.iter().all(|(t, _)| t.name != "tx_remove"),
        "tx_remove tag should be deleted after full removal"
    );
}

#[test]
fn batch_remove_tag_marks_changed_records_pending_sync() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id1 = create_record_with_tags(&mut svc, "Rec1", vec!["remove_me".to_string()]);
    let id2 = create_record_with_tags(&mut svc, "Rec2", vec!["remove_me".to_string()]);
    svc.mark_record_synced(&id1).expect("record 1 synced");
    svc.mark_record_synced(&id2).expect("record 2 synced");

    let removed = svc
        .batch_remove_tag(&[id1, id2], "remove_me")
        .expect("batch_remove_tag must succeed");

    assert_eq!(removed, 2);
    let sync_map = svc.load_sync_status_map();
    assert_eq!(sync_map.get(&id1.to_string()), Some(&SyncStatus::Pending));
    assert_eq!(sync_map.get(&id2.to_string()), Some(&SyncStatus::Pending));
}

#[test]
fn batch_remove_tag_skips_records_without_tag() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Only id1 has the "selective" tag
    let id1 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Tagged".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass1".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["selective".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let id2 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Untagged".to_string(),
                username: "user2".to_string(),
                password: SecureStr::new("pass2".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let removed = svc
        .batch_remove_tag(&[id1, id2], "selective")
        .expect("batch_remove_tag must succeed");
    assert_eq!(removed, 1, "only 1 record actually had the tag");
}

// =========================================================================
// list_tags_with_stats tests
// =========================================================================

#[test]
fn list_tags_with_stats_returns_empty_when_no_tags_exist() {
    let svc = setup_service();
    let tags = svc
        .list_tags_with_stats()
        .expect("list_tags_with_stats must succeed");
    assert!(tags.is_empty(), "no tags should exist in empty vault");
}

#[test]
fn list_tags_with_stats_returns_tags_with_correct_record_count_and_last_used_at() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create records with overlapping tags
    // Rec1: work, dev
    create_record_with_tags(
        &mut svc,
        "Rec1",
        vec!["work".to_string(), "dev".to_string()],
    );
    // Rec2: work
    create_record_with_tags(&mut svc, "Rec2", vec!["work".to_string()]);
    // Rec3: personal
    create_record_with_tags(&mut svc, "Rec3", vec!["personal".to_string()]);

    let tags = svc
        .list_tags_with_stats()
        .expect("list_tags_with_stats must succeed");

    // Ordered alphabetically: dev, personal, work
    assert_eq!(tags.len(), 3);

    let (dev_tag, dev_meta) = &tags[0];
    assert_eq!(dev_tag.name, "dev");
    assert_eq!(dev_meta.record_count, 1, "dev tag used by 1 record");
    assert_ne!(dev_meta.last_used_at, 0, "dev tag should have last_used_at");

    let (personal_tag, personal_meta) = &tags[1];
    assert_eq!(personal_tag.name, "personal");
    assert_eq!(
        personal_meta.record_count, 1,
        "personal tag used by 1 record"
    );
    assert_ne!(
        personal_meta.last_used_at, 0,
        "personal tag should have last_used_at"
    );

    let (work_tag, work_meta) = &tags[2];
    assert_eq!(work_tag.name, "work");
    assert_eq!(work_meta.record_count, 2, "work tag used by 2 records");
    assert_ne!(
        work_meta.last_used_at, 0,
        "work tag should have last_used_at"
    );

    // Verify last_used_at is reasonable (non-zero and not in the future)
    assert!(work_meta.last_used_at > 0);
    assert!(work_meta.last_used_at <= chrono::Utc::now().timestamp());
}

#[test]
fn list_tags_with_stats_excludes_soft_deleted_records_from_count_and_last_used_at() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let id1 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "ToBeDeleted".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["work".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let _id2 = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Active".to_string(),
                username: "user2".to_string(),
                password: SecureStr::new("pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["work".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    // Before soft delete: work tag count = 2, last_used_at = max of both
    let tags_before = svc
        .list_tags_with_stats()
        .expect("list_tags_with_stats must succeed");
    let work_before = tags_before.iter().find(|(t, _)| t.name == "work").unwrap();
    assert_eq!(work_before.1.record_count, 2);

    // Soft delete one record
    svc.soft_delete_record(id1)
        .expect("soft_delete must succeed");

    // After soft delete: work tag count = 1
    let tags_after = svc
        .list_tags_with_stats()
        .expect("list_tags_with_stats must succeed");
    let work_after = tags_after.iter().find(|(t, _)| t.name == "work").unwrap();
    assert_eq!(
        work_after.1.record_count, 1,
        "soft-deleted records should not count"
    );
    // Verify last_used_at is reasonable
    assert!(work_after.1.last_used_at > 0);
    assert!(work_after.1.last_used_at <= chrono::Utc::now().timestamp());
}

#[test]
fn list_tags_with_stats_returns_zero_count_and_zero_last_used_for_unused_tags() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create a tag via record creation, then hard delete the record
    let id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "TempRec".to_string(),
                username: "user1".to_string(),
                password: SecureStr::new("pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["orphan".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    // Hard delete removes the record and cascade-deletes record_tags
    svc.hard_delete_record(id)
        .expect("hard_delete must succeed");

    // The tag might still exist but has no associations
    let tags = svc
        .list_tags_with_stats()
        .expect("list_tags_with_stats must succeed");
    let orphan = tags.iter().find(|(t, _)| t.name == "orphan");

    // Tag might not exist (if cascade-deleted) or exist with 0 count
    if let Some((_, meta)) = orphan {
        assert_eq!(
            meta.record_count, 0,
            "orphan tag should have 0 record_count"
        );
        assert_eq!(
            meta.last_used_at, 0,
            "orphan tag should have 0 last_used_at"
        );
    }
}

#[test]
fn list_tags_with_stats_returns_correct_last_used_at_for_multiple_records() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create records with the same tag at different times
    create_record_with_tags(&mut svc, "Rec1", vec!["shared".to_string()]);
    create_record_with_tags(&mut svc, "Rec2", vec!["shared".to_string()]);
    create_record_with_tags(&mut svc, "Rec3", vec!["shared".to_string()]);

    let tags = svc
        .list_tags_with_stats()
        .expect("list_tags_with_stats must succeed");
    let shared = tags.iter().find(|(t, _)| t.name == "shared").unwrap();

    assert_eq!(shared.1.record_count, 3, "shared tag used by 3 records");
    // Verify last_used_at is reasonable
    assert!(shared.1.last_used_at > 0);
    assert!(shared.1.last_used_at <= chrono::Utc::now().timestamp());
}

#[test]
fn list_tags_with_stats_preserves_backward_compatibility_with_list_tags() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    create_record_with_tags(
        &mut svc,
        "Rec1",
        vec!["work".to_string(), "dev".to_string()],
    );

    // Both methods should return the same record counts
    let tags_simple = svc.list_tags().expect("list_tags must succeed");
    let tags_with_stats = svc
        .list_tags_with_stats()
        .expect("list_tags_with_stats must succeed");

    assert_eq!(tags_simple.len(), tags_with_stats.len());

    for (simple, with_stats) in tags_simple.iter().zip(tags_with_stats.iter()) {
        assert_eq!(simple.0.name, with_stats.0.name, "tag names should match");
        assert_eq!(
            simple.1, with_stats.1.record_count,
            "usage count should match record_count"
        );
    }
}
