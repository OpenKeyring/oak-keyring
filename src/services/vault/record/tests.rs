use chrono::Utc;
use uuid::Uuid;

use crate::commands::types::{FieldSelector, RecordFilter, RecordSort, SortDirection, SortField};
use crate::crypto::bip39::{MnemonicLanguage, Passkey};
use crate::crypto::payload;
use crate::db::queries;
use crate::db::schema::{initialize_metadata, initialize_schema};
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::VaultService;
use crate::types::audit::AuditOperation;
use crate::types::credential::{CredentialType, EncryptedPayload};
use crate::types::record::{CreateRecordParams, DecryptedRecord, UpdateRecordParams};
use crate::types::sensitive::SecureStr;
use rusqlite::Connection;

/// Helper: create an in-memory VaultService with schema initialized.
fn setup_service() -> VaultService {
    let conn = Connection::open_in_memory().unwrap();
    initialize_schema(&conn);
    initialize_metadata(&conn);
    VaultService::new(conn)
}

/// Helper: unlock the VaultService with a fresh mnemonic.
fn unlock_service(svc: &mut VaultService) {
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
    svc.crypto
        .unlock_with_mnemonic(&mnemonic)
        .expect("unlock_with_mnemonic must succeed in test");
}

/// Helper: create a Login-type EncryptedPayload for testing.
fn sample_login_payload(name: &str) -> EncryptedPayload {
    EncryptedPayload::Login {
        name: name.to_string(),
        username: "alice".to_string(),
        password: SecureStr::new("s3cret!".to_string()),
        url: Some("https://github.com".to_string()),
        notes: None,
    }
}

// --- NotUnlocked guard ---

#[test]
fn create_record_returns_not_unlocked_when_locked() {
    let mut svc = setup_service();
    assert!(!svc.is_unlocked(), "service must start locked");

    let params = CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: sample_login_payload("Test"),
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    };

    let result = svc.create_record(params);
    assert!(result.is_err(), "create_record must fail when not unlocked");
    assert!(
        matches!(result.unwrap_err(), VaultError::NotUnlocked),
        "expected NotUnlocked error"
    );
}

// --- Successful creation with tags and retrieval ---

#[test]
fn create_login_record_and_retrieve_via_queries() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let params = CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: sample_login_payload("GitHub"),
        tags: vec!["work".to_string(), "dev".to_string()],
        is_favorite: true,
        expires_at: None,
    };

    let id = svc
        .create_record(params)
        .expect("create_record must succeed");

    // Verify record exists via queries::get_record
    let stored = queries::get_record(&svc.conn, &id)
        .expect("get_record query must succeed")
        .expect("record must exist in DB");

    assert_eq!(stored.id, id);
    assert_eq!(stored.credential_type, CredentialType::Login);
    assert!(stored.is_favorite);
    assert_eq!(stored.version, 1);
    assert!(!stored.deleted);
    // Tags are stored but may not be in insertion order (depends on DB indexing)
    let mut sorted_tags = stored.tags.clone();
    sorted_tags.sort();
    assert_eq!(sorted_tags, vec!["dev", "work"]);

    // Verify AAD is stored correctly
    let expected_aad = format!("record:{}", id);
    assert_eq!(stored.aad, expected_aad.as_bytes());
}

// --- Audit log verification ---

#[test]
fn create_record_writes_audit_entry() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let params = CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: sample_login_payload("MySite"),
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    };

    let id = svc
        .create_record(params)
        .expect("create_record must succeed");

    // Verify audit entry
    let audit_entries =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");

    assert_eq!(audit_entries.len(), 1, "expected exactly one audit entry");
    let entry = &audit_entries[0];
    assert_eq!(entry.operation, AuditOperation::RecordCreate);
    assert_eq!(entry.record_id, Some(id));
    assert_eq!(entry.record_name.as_deref(), Some("MySite"));
}

// --- Returned UUID is valid ---

#[test]
fn create_record_returns_valid_uuid() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let params = CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: sample_login_payload("UUID Test"),
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    };

    let id = svc
        .create_record(params)
        .expect("create_record must succeed");

    // Verify it is a valid UUID v4
    assert_eq!(id.get_version(), Some(uuid::Version::Random));
}

// --- Encrypted data is not empty and nonce is 24 bytes ---

#[test]
fn create_record_stores_encrypted_data_and_nonce() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let params = CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: sample_login_payload("Encrypted Check"),
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    };

    let id = svc
        .create_record(params)
        .expect("create_record must succeed");

    let stored = queries::get_record(&svc.conn, &id)
        .expect("query must succeed")
        .expect("record must exist");

    assert!(
        !stored.encrypted_data.is_empty(),
        "encrypted_data must not be empty"
    );
    assert_eq!(stored.nonce.len(), 24, "nonce must be 24 bytes");
}

// --- DEK version is stored correctly ---

#[test]
fn create_record_stores_correct_dek_version() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let params = CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: sample_login_payload("DEK Test"),
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    };

    let id = svc
        .create_record(params)
        .expect("create_record must succeed");

    let stored = queries::get_record(&svc.conn, &id)
        .expect("query must succeed")
        .expect("record must exist");

    assert_eq!(stored.dek_version, svc.crypto.current_dek_version());
}

// --- Roundtrip: create then decrypt ---

#[test]
fn create_record_roundtrip_encrypt_decrypt() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let params = CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: sample_login_payload("Roundtrip"),
        tags: vec!["test".to_string()],
        is_favorite: false,
        expires_at: None,
    };

    let id = svc
        .create_record(params)
        .expect("create_record must succeed");

    let stored = queries::get_record(&svc.conn, &id)
        .expect("query must succeed")
        .expect("record must exist");

    // Decrypt using the same CryptoManager
    let aad = format!("record:{}", id);
    let decrypted = payload::decrypt_payload(
        &svc.crypto,
        &stored.encrypted_data,
        &stored.nonce,
        aad.as_bytes(),
        stored.credential_type,
        stored.dek_version,
    )
    .expect("decryption must succeed");

    assert_eq!(decrypted.name(), "Roundtrip");
    assert_eq!(decrypted.credential_type(), CredentialType::Login);
}

// =========================================================================
// get_stored_record tests
// =========================================================================

// --- get_stored_record: nonexistent UUID returns RecordNotFound ---

#[test]
fn get_stored_record_returns_not_found_for_nonexistent_uuid() {
    let svc = setup_service();
    let nonexistent = Uuid::new_v4();

    let result = svc.get_stored_record(nonexistent);
    assert!(
        result.is_err(),
        "get_stored_record must fail for nonexistent ID"
    );
    assert!(
        matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
        "expected RecordNotFound with the given UUID"
    );
}

// --- get_stored_record: returns tags matching creation ---

#[test]
fn get_stored_record_returns_tags_matching_creation() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let params = CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: sample_login_payload("TagCheck"),
        tags: vec!["alpha".to_string(), "beta".to_string()],
        is_favorite: true,
        expires_at: None,
    };

    let id = svc
        .create_record(params)
        .expect("create_record must succeed");

    let stored = svc
        .get_stored_record(id)
        .expect("get_stored_record must succeed");

    assert_eq!(stored.id, id);
    let mut sorted_tags = stored.tags.clone();
    sorted_tags.sort();
    assert_eq!(sorted_tags, vec!["alpha", "beta"]);
}

// =========================================================================
// get_decrypted_record tests
// =========================================================================

// --- get_decrypted_record: NotUnlocked guard ---

#[test]
fn get_decrypted_record_returns_not_unlocked_when_locked() {
    let mut svc = setup_service();
    // Not unlocked — service starts locked

    let result = svc.get_decrypted_record(Uuid::new_v4());
    assert!(
        result.is_err(),
        "get_decrypted_record must fail when locked"
    );
    assert!(
        matches!(result.unwrap_err(), VaultError::NotUnlocked),
        "expected NotUnlocked error"
    );
}

// --- get_decrypted_record: decrypts Login username/password matching creation ---

#[test]
fn get_decrypted_record_decrypts_login_credentials() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let params = CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: sample_login_payload("DecryptTest"),
        tags: vec!["secure".to_string()],
        is_favorite: false,
        expires_at: None,
    };

    let id = svc
        .create_record(params)
        .expect("create_record must succeed");

    let decrypted = svc
        .get_decrypted_record(id)
        .expect("get_decrypted_record must succeed");

    match decrypted {
        DecryptedRecord::Login {
            name,
            username,
            password,
            url,
            notes,
            tags,
            is_favorite,
            ..
        } => {
            assert_eq!(name, "DecryptTest");
            assert_eq!(username, "alice");
            assert_eq!(password.get(), "s3cret!");
            assert_eq!(url.as_deref(), Some("https://github.com"));
            assert!(notes.is_none());
            assert_eq!(tags, vec!["secure"]);
            assert!(!is_favorite);
        }
        other => panic!("expected DecryptedRecord::Login, got {:?}", other),
    }
}

// --- get_decrypted_record: writes audit RecordViewPassword ---

#[test]
fn get_decrypted_record_writes_audit_record_view_password() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let params = CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: sample_login_payload("AuditCheck"),
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    };

    let id = svc
        .create_record(params)
        .expect("create_record must succeed");

    // One audit entry from create_record
    let before =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(
        before.len(),
        1,
        "expected one audit entry from create_record"
    );

    svc.get_decrypted_record(id)
        .expect("get_decrypted_record must succeed");

    // Now two audit entries: RecordCreate + RecordViewPassword
    let after =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(
        after.len(),
        2,
        "expected two audit entries after get_decrypted_record"
    );

    // Find the RecordViewPassword entry (order may vary if timestamps collide)
    let view_entry = after
        .iter()
        .find(|e| e.operation == AuditOperation::RecordViewPassword)
        .expect("expected a RecordViewPassword audit entry");
    assert_eq!(view_entry.record_id, Some(id));
    assert_eq!(view_entry.record_name.as_deref(), Some("AuditCheck"));
}

// --- get_decrypted_record: nonexistent UUID returns RecordNotFound ---

#[test]
fn get_decrypted_record_returns_not_found_for_nonexistent_uuid() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let nonexistent = Uuid::new_v4();

    let result = svc.get_decrypted_record(nonexistent);
    assert!(
        result.is_err(),
        "get_decrypted_record must fail for nonexistent ID"
    );
    assert!(
        matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
        "expected RecordNotFound with the given UUID"
    );
}

// =========================================================================
// update_record tests
// =========================================================================

/// Helper: create a Login record and return its ID.
fn create_test_login_record(svc: &mut VaultService) -> Uuid {
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: sample_login_payload("TestLogin"),
        tags: vec!["work".to_string()],
        is_favorite: false,
        expires_at: None,
    })
    .expect("create_record must succeed")
}

// --- update_record: NotUnlocked guard ---

#[test]
fn update_record_returns_not_unlocked_when_locked() {
    let mut svc = setup_service();
    assert!(!svc.is_unlocked());

    let params = UpdateRecordParams {
        id: Uuid::new_v4(),
        payload: sample_login_payload("Test"),
        tags: vec![],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    };

    let result = svc.update_record(params);
    assert!(result.is_err(), "update_record must fail when locked");
    assert!(
        matches!(result.unwrap_err(), VaultError::NotUnlocked),
        "expected NotUnlocked error"
    );
}

// --- update_record: version mismatch returns VersionConflict ---

#[test]
fn update_record_returns_version_conflict_on_mismatch() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    let params = UpdateRecordParams {
        id,
        payload: sample_login_payload("Updated"),
        tags: vec![],
        is_favorite: false,
        expires_at: None,
        expected_version: 99, // Wrong version
    };

    let result = svc.update_record(params);
    assert!(
        result.is_err(),
        "update_record must fail on version mismatch"
    );
    match result.unwrap_err() {
        VaultError::VersionConflict { expected, actual } => {
            assert_eq!(expected, 99);
            assert_eq!(actual, 1);
        }
        other => panic!("expected VersionConflict, got {:?}", other),
    }
}

// --- update_record: password change creates password history entry ---

#[test]
fn update_record_saves_password_history_when_password_changes() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Verify no history before update
    let count_before = queries::count_password_history(&svc.conn, &id).unwrap();
    assert_eq!(count_before, 0, "no history before update");

    // Update with a different password
    let new_payload = EncryptedPayload::Login {
        name: "TestLogin".to_string(),
        username: "alice".to_string(),
        password: SecureStr::new("newP@ssw0rd!".to_string()),
        url: Some("https://github.com".to_string()),
        notes: None,
    };

    let params = UpdateRecordParams {
        id,
        payload: new_payload,
        tags: vec!["work".to_string()],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    };

    svc.update_record(params)
        .expect("update_record must succeed");

    // Verify password history has one entry
    let count_after = queries::count_password_history(&svc.conn, &id).unwrap();
    assert_eq!(count_after, 1, "one history entry after password change");
}

// --- update_record: no history when password unchanged ---

#[test]
fn update_record_skips_password_history_when_password_unchanged() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Update with the same password (only name changes)
    let new_payload = EncryptedPayload::Login {
        name: "TestLoginRenamed".to_string(),
        username: "alice".to_string(),
        password: SecureStr::new("s3cret!".to_string()), // Same password
        url: Some("https://github.com".to_string()),
        notes: None,
    };

    let params = UpdateRecordParams {
        id,
        payload: new_payload,
        tags: vec![],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    };

    svc.update_record(params)
        .expect("update_record must succeed");

    let count = queries::count_password_history(&svc.conn, &id).unwrap();
    assert_eq!(count, 0, "no history when password unchanged");
}

// --- update_record: tags are replaced ---

#[test]
fn update_record_replaces_tags() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Verify initial tags
    let stored_before = svc.get_stored_record(id).unwrap();
    assert_eq!(stored_before.tags, vec!["work"]);

    // Update with different tags
    let params = UpdateRecordParams {
        id,
        payload: sample_login_payload("TestLogin"),
        tags: vec!["personal".to_string(), "email".to_string()],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    };

    svc.update_record(params)
        .expect("update_record must succeed");

    let stored_after = svc.get_stored_record(id).unwrap();
    let mut tags = stored_after.tags.clone();
    tags.sort();
    assert_eq!(tags, vec!["email", "personal"]);
}

// --- update_record: audit log contains RecordUpdate ---

#[test]
fn update_record_writes_audit_entry() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // One audit entry from create
    let before =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(before.len(), 1);

    let params = UpdateRecordParams {
        id,
        payload: sample_login_payload("TestLoginRenamed"),
        tags: vec![],
        is_favorite: true,
        expires_at: None,
        expected_version: 1,
    };

    svc.update_record(params)
        .expect("update_record must succeed");

    let after =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(after.len(), 2, "expected two audit entries after update");

    let update_entry = after
        .iter()
        .find(|e| e.operation == AuditOperation::RecordUpdate)
        .expect("expected a RecordUpdate audit entry");
    assert_eq!(update_entry.record_id, Some(id));
    assert_eq!(
        update_entry.record_name.as_deref(),
        Some("TestLoginRenamed")
    );
}

// --- update_record: version is incremented ---

#[test]
fn update_record_increments_version() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    let stored_before = svc.get_stored_record(id).unwrap();
    assert_eq!(stored_before.version, 1);

    let params = UpdateRecordParams {
        id,
        payload: sample_login_payload("TestLoginV2"),
        tags: vec![],
        is_favorite: true,
        expires_at: None,
        expected_version: 1,
    };

    svc.update_record(params)
        .expect("update_record must succeed");

    let stored_after = svc.get_stored_record(id).unwrap();
    assert_eq!(stored_after.version, 2);
    assert!(stored_after.is_favorite);
}

// --- update_record: encrypted payload roundtrips ---

#[test]
fn update_record_payload_decrypts_correctly() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    let new_payload = EncryptedPayload::Login {
        name: "UpdatedSite".to_string(),
        username: "bob".to_string(),
        password: SecureStr::new("n3wP@ss".to_string()),
        url: None,
        notes: Some("updated notes".to_string()),
    };

    let params = UpdateRecordParams {
        id,
        payload: new_payload,
        tags: vec![],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    };

    svc.update_record(params)
        .expect("update_record must succeed");

    // Decrypt and verify
    let decrypted = svc.get_decrypted_record(id).expect("must decrypt");
    match decrypted {
        DecryptedRecord::Login {
            name,
            username,
            password,
            url,
            notes,
            ..
        } => {
            assert_eq!(name, "UpdatedSite");
            assert_eq!(username, "bob");
            assert_eq!(password.get(), "n3wP@ss");
            assert!(url.is_none());
            assert_eq!(notes.as_deref(), Some("updated notes"));
        }
        other => panic!("expected Login, got {:?}", other),
    }
}

// --- update_record: nonexistent record returns RecordNotFound ---

#[test]
fn update_record_returns_not_found_for_nonexistent() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let nonexistent = Uuid::new_v4();

    let params = UpdateRecordParams {
        id: nonexistent,
        payload: sample_login_payload("Ghost"),
        tags: vec![],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    };

    let result = svc.update_record(params);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
        "expected RecordNotFound"
    );
}

// --- update_record: double update (consecutive version bumps) ---

#[test]
fn update_record_supports_consecutive_updates() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // First update: v1 -> v2
    let params_v2 = UpdateRecordParams {
        id,
        payload: sample_login_payload("V2"),
        tags: vec![],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    };
    svc.update_record(params_v2)
        .expect("first update must succeed");

    // Second update: v2 -> v3
    let params_v3 = UpdateRecordParams {
        id,
        payload: sample_login_payload("V3"),
        tags: vec!["final".to_string()],
        is_favorite: true,
        expires_at: None,
        expected_version: 2,
    };
    svc.update_record(params_v3)
        .expect("second update must succeed");

    let stored = svc.get_stored_record(id).unwrap();
    assert_eq!(stored.version, 3);
    assert!(stored.is_favorite);
    assert_eq!(stored.tags, vec!["final"]);
}

// =========================================================================
// soft_delete_record tests
// =========================================================================

// --- soft_delete_record: NotUnlocked guard ---

#[test]
fn soft_delete_record_returns_not_unlocked_when_locked() {
    let mut svc = setup_service();
    assert!(!svc.is_unlocked());

    let result = svc.soft_delete_record(Uuid::new_v4());
    assert!(result.is_err(), "soft_delete_record must fail when locked");
    assert!(
        matches!(result.unwrap_err(), VaultError::NotUnlocked),
        "expected NotUnlocked error"
    );
}

// --- soft_delete_record: record not in list_records(All) after soft delete ---

#[test]
fn soft_delete_record_removes_from_active_listing() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Record is in active listing before soft delete
    let active_before = queries::list_active_records(&svc.conn).unwrap();
    assert_eq!(active_before.len(), 1);

    svc.soft_delete_record(id)
        .expect("soft_delete_record must succeed");

    // Record is NOT in active listing after soft delete
    let active_after = queries::list_active_records(&svc.conn).unwrap();
    assert!(
        active_after.is_empty(),
        "soft-deleted record must not appear in active listing"
    );

    // Record still exists in DB with deleted = true
    let stored = svc.get_stored_record(id).unwrap();
    assert!(stored.deleted);
    assert!(stored.deleted_at.is_some());
}

// --- soft_delete_record: writes audit RecordDelete ---

#[test]
fn soft_delete_record_writes_audit_record_delete() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // One audit entry from create
    let before =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(before.len(), 1);

    svc.soft_delete_record(id)
        .expect("soft_delete_record must succeed");

    let after =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(
        after.len(),
        2,
        "expected two audit entries after soft delete"
    );

    let delete_entry = after
        .iter()
        .find(|e| e.operation == AuditOperation::RecordDelete)
        .expect("expected a RecordDelete audit entry");
    assert_eq!(delete_entry.record_id, Some(id));
    assert_eq!(delete_entry.record_name.as_deref(), Some("TestLogin"));
}

// --- soft_delete_record: nonexistent record returns RecordNotFound ---

#[test]
fn soft_delete_record_returns_not_found_for_nonexistent() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let nonexistent = Uuid::new_v4();

    let result = svc.soft_delete_record(nonexistent);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
        "expected RecordNotFound"
    );
}

// =========================================================================
// restore_record tests
// =========================================================================

// --- restore_record: NotUnlocked guard ---

#[test]
fn restore_record_returns_not_unlocked_when_locked() {
    let mut svc = setup_service();
    assert!(!svc.is_unlocked());

    let result = svc.restore_record(Uuid::new_v4());
    assert!(result.is_err(), "restore_record must fail when locked");
    assert!(
        matches!(result.unwrap_err(), VaultError::NotUnlocked),
        "expected NotUnlocked error"
    );
}

// --- restore_record: record back in list_records(All) after restore ---

#[test]
fn restore_record_returns_to_active_listing() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Soft delete first
    svc.soft_delete_record(id)
        .expect("soft_delete_record must succeed");
    let after_delete = queries::list_active_records(&svc.conn).unwrap();
    assert!(after_delete.is_empty());

    // Restore
    svc.restore_record(id).expect("restore_record must succeed");

    // Record is back in active listing
    let after_restore = queries::list_active_records(&svc.conn).unwrap();
    assert_eq!(
        after_restore.len(),
        1,
        "restored record must appear in active listing"
    );
    assert_eq!(after_restore[0].id, id);

    // Record fields reflect non-deleted state
    let stored = svc.get_stored_record(id).unwrap();
    assert!(!stored.deleted);
    assert!(stored.deleted_at.is_none());
}

// --- restore_record: writes audit RecordRestore ---

#[test]
fn restore_record_writes_audit_record_restore() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    svc.soft_delete_record(id)
        .expect("soft_delete_record must succeed");

    // Two audit entries: create + soft_delete
    let before_restore =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(before_restore.len(), 2);

    svc.restore_record(id).expect("restore_record must succeed");

    let after =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(after.len(), 3, "expected three audit entries after restore");

    let restore_entry = after
        .iter()
        .find(|e| e.operation == AuditOperation::RecordRestore)
        .expect("expected a RecordRestore audit entry");
    assert_eq!(restore_entry.record_id, Some(id));
    assert_eq!(restore_entry.record_name.as_deref(), Some("TestLogin"));
}

// --- restore_record: nonexistent record returns RecordNotFound ---

#[test]
fn restore_record_returns_not_found_for_nonexistent() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let nonexistent = Uuid::new_v4();

    let result = svc.restore_record(nonexistent);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
        "expected RecordNotFound"
    );
}

// =========================================================================
// hard_delete_record tests
// =========================================================================

// --- hard_delete_record: NotUnlocked guard ---

#[test]
fn hard_delete_record_returns_not_unlocked_when_locked() {
    let mut svc = setup_service();
    assert!(!svc.is_unlocked());

    let result = svc.hard_delete_record(Uuid::new_v4());
    assert!(result.is_err(), "hard_delete_record must fail when locked");
    assert!(
        matches!(result.unwrap_err(), VaultError::NotUnlocked),
        "expected NotUnlocked error"
    );
}

// --- hard_delete_record: cascade deletes record_tags and password_history ---

#[test]
fn hard_delete_record_cascades_deletes_tags_and_password_history() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create a record with tags
    let id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("CascadeTest"),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    // Create password history by updating with a new password
    let new_payload = EncryptedPayload::Login {
        name: "CascadeTest".to_string(),
        username: "alice".to_string(),
        password: SecureStr::new("newPassword123!".to_string()),
        url: Some("https://github.com".to_string()),
        notes: None,
    };
    svc.update_record(UpdateRecordParams {
        id,
        payload: new_payload,
        tags: vec!["tag1".to_string()],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    })
    .expect("update_record must succeed");

    // Verify password history exists
    let history_count = queries::count_password_history(&svc.conn, &id).unwrap();
    assert_eq!(history_count, 1, "one password history entry should exist");

    // Verify tags exist
    let tags = queries::get_record_tags(&svc.conn, &id).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0], "tag1");

    // Hard delete
    svc.hard_delete_record(id)
        .expect("hard_delete_record must succeed");

    // Record is gone
    let record_result = queries::get_record(&svc.conn, &id).unwrap();
    assert!(
        record_result.is_none(),
        "record must be gone after hard delete"
    );

    // Password history cascade deleted
    let history_after = queries::count_password_history(&svc.conn, &id).unwrap();
    assert_eq!(history_after, 0, "password history must be cascade deleted");

    // Record tags cascade deleted
    let tags_after = queries::get_record_tags(&svc.conn, &id).unwrap();
    assert!(tags_after.is_empty(), "record_tags must be cascade deleted");
}

// --- hard_delete_record: writes audit RecordDestroy ---

#[test]
fn hard_delete_record_writes_audit_record_destroy() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // One audit entry from create
    let before =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(before.len(), 1);

    svc.hard_delete_record(id)
        .expect("hard_delete_record must succeed");

    let after =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(
        after.len(),
        2,
        "expected two audit entries after hard delete"
    );

    let destroy_entry = after
        .iter()
        .find(|e| e.operation == AuditOperation::RecordDestroy)
        .expect("expected a RecordDestroy audit entry");
    assert_eq!(destroy_entry.record_id, Some(id));
    assert_eq!(destroy_entry.record_name.as_deref(), Some("TestLogin"));
}

// --- hard_delete_record: nonexistent record returns RecordNotFound ---

#[test]
fn hard_delete_record_returns_not_found_for_nonexistent() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let nonexistent = Uuid::new_v4();

    let result = svc.hard_delete_record(nonexistent);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
        "expected RecordNotFound"
    );
}

// =========================================================================
// toggle_favorite tests
// =========================================================================

// --- toggle_favorite: NotUnlocked guard ---

#[test]
fn toggle_favorite_returns_not_unlocked_when_locked() {
    let mut svc = setup_service();
    assert!(!svc.is_unlocked());

    let result = svc.toggle_favorite(Uuid::new_v4(), true);
    assert!(result.is_err(), "toggle_favorite must fail when locked");
    assert!(
        matches!(result.unwrap_err(), VaultError::NotUnlocked),
        "expected NotUnlocked error"
    );
}

// --- toggle_favorite: changes is_favorite but not version ---

#[test]
fn toggle_favorite_changes_favorite_without_version_increment() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Initially not favorite, version 1
    let before = svc.get_stored_record(id).unwrap();
    assert!(!before.is_favorite);
    assert_eq!(before.version, 1);

    // Set to favorite
    svc.toggle_favorite(id, true)
        .expect("toggle_favorite must succeed");

    let after = svc.get_stored_record(id).unwrap();
    assert!(
        after.is_favorite,
        "is_favorite should be true after toggle_favorite(true)"
    );
    assert_eq!(
        after.version, 1,
        "version must NOT increment on toggle_favorite"
    );

    // Set back to not favorite
    svc.toggle_favorite(id, false)
        .expect("toggle_favorite must succeed");

    let after_unset = svc.get_stored_record(id).unwrap();
    assert!(
        !after_unset.is_favorite,
        "is_favorite should be false after toggle_favorite(false)"
    );
    assert_eq!(after_unset.version, 1, "version must still not increment");
}

// --- toggle_favorite: no audit entry written ---

#[test]
fn toggle_favorite_does_not_write_audit_entry() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // One audit entry from create
    let before =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(before.len(), 1);

    svc.toggle_favorite(id, true)
        .expect("toggle_favorite must succeed");

    // Still only one audit entry — no new audit for toggle_favorite
    let after =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(
        after.len(),
        1,
        "toggle_favorite must not write an audit entry"
    );
}

// --- toggle_favorite: nonexistent record returns RecordNotFound ---

#[test]
fn toggle_favorite_returns_not_found_for_nonexistent() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let nonexistent = Uuid::new_v4();

    let result = svc.toggle_favorite(nonexistent, true);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
        "expected RecordNotFound"
    );
}

// =========================================================================
// list_records tests
// =========================================================================

/// Helper: create a Login record with a specific name and return its ID.
fn create_named_record(svc: &mut VaultService, name: &str) -> Uuid {
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: EncryptedPayload::Login {
            name: name.to_string(),
            username: format!("user_{}", name.to_lowercase()),
            password: SecureStr::new("password123".to_string()),
            url: None,
            notes: None,
        },
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    })
    .expect("create_record must succeed")
}

// --- list_records: NotUnlocked guard ---

#[test]
fn list_records_returns_not_unlocked_when_locked() {
    let svc = setup_service();
    assert!(!svc.is_unlocked());

    let result = svc.list_records(
        &RecordFilter::All,
        &RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        },
    );
    assert!(result.is_err(), "list_records must fail when locked");
    assert!(
        matches!(result.unwrap_err(), VaultError::NotUnlocked),
        "expected NotUnlocked error"
    );
}

// --- list_records: All filter returns all active records ---

#[test]
fn list_records_all_returns_all_active_records() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let _id_a = create_named_record(&mut svc, "Alpha");
    let _id_b = create_named_record(&mut svc, "Bravo");
    let _id_c = create_named_record(&mut svc, "Charlie");

    let sort = RecordSort {
        field: SortField::UpdatedAt,
        direction: SortDirection::Desc,
    };

    let records = svc
        .list_records(&RecordFilter::All, &sort)
        .expect("list_records must succeed");

    assert_eq!(records.len(), 3, "should return 3 active records");
}

// --- list_records: sort by UpdatedAt Desc uses correct ordering ---

#[test]
fn list_records_sorts_by_updated_at_desc() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let id_first = create_named_record(&mut svc, "First");
    let _id_second = create_named_record(&mut svc, "Second");
    let _id_third = create_named_record(&mut svc, "Third");

    // Update id_first to give it the most recent updated_at
    svc.update_record(UpdateRecordParams {
        id: id_first,
        payload: EncryptedPayload::Login {
            name: "FirstUpdated".to_string(),
            username: "user_first".to_string(),
            password: SecureStr::new("password123".to_string()),
            url: None,
            notes: None,
        },
        tags: vec![],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    })
    .expect("update_record must succeed");

    let sort_desc = RecordSort {
        field: SortField::UpdatedAt,
        direction: SortDirection::Desc,
    };

    let records = svc
        .list_records(&RecordFilter::All, &sort_desc)
        .expect("list_records must succeed");

    assert_eq!(records.len(), 3);
    // id_first was updated last, so it should come first in DESC order
    assert_eq!(records[0].id, id_first);
}

// --- list_records: sort by UpdatedAt Asc ---

#[test]
fn list_records_sorts_by_updated_at_asc() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let _id_first = create_named_record(&mut svc, "First");
    let _id_second = create_named_record(&mut svc, "Second");
    let id_third = create_named_record(&mut svc, "Third");

    // Update id_third to give it the most recent updated_at
    svc.update_record(UpdateRecordParams {
        id: id_third,
        payload: EncryptedPayload::Login {
            name: "ThirdUpdated".to_string(),
            username: "user_third".to_string(),
            password: SecureStr::new("password123".to_string()),
            url: None,
            notes: None,
        },
        tags: vec![],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    })
    .expect("update_record must succeed");

    let sort_asc = RecordSort {
        field: SortField::UpdatedAt,
        direction: SortDirection::Asc,
    };

    let records = svc
        .list_records(&RecordFilter::All, &sort_asc)
        .expect("list_records must succeed");

    assert_eq!(records.len(), 3);
    // id_third was updated last, so it should come last in ASC order
    assert_eq!(records[2].id, id_third);
}

// --- list_records: Favorites filter returns only is_favorite records ---

#[test]
fn list_records_favorites_returns_only_favorites() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create two non-favorite records
    let _id_a = create_named_record(&mut svc, "Alpha");
    let _id_b = create_named_record(&mut svc, "Bravo");

    // Create a favorite record
    let id_fav = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "FavoriteSite".to_string(),
                username: "fav_user".to_string(),
                password: SecureStr::new("fav_pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: true,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let sort = RecordSort {
        field: SortField::UpdatedAt,
        direction: SortDirection::Desc,
    };

    let favorites = svc
        .list_records(&RecordFilter::Favorites, &sort)
        .expect("list_records must succeed");

    assert_eq!(favorites.len(), 1, "only favorite records returned");
    assert_eq!(favorites[0].id, id_fav);
    assert!(favorites[0].is_favorite);
}

// --- list_records: Tag filter returns only records with that tag ---

#[test]
fn list_records_tag_filter_returns_matching_records() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create a record with "work" tag
    let id_work = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "WorkSite".to_string(),
                username: "work_user".to_string(),
                password: SecureStr::new("work_pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["work".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    // Create a record with "personal" tag
    let id_personal = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "PersonalSite".to_string(),
                username: "personal_user".to_string(),
                password: SecureStr::new("personal_pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["personal".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");

    // Create an untagged record
    let _id_untagged = create_named_record(&mut svc, "Untagged");

    let sort = RecordSort {
        field: SortField::UpdatedAt,
        direction: SortDirection::Desc,
    };

    let work_records = svc
        .list_records(&RecordFilter::Tag("work".into()), &sort)
        .expect("list_records must succeed");
    assert_eq!(work_records.len(), 1);
    assert_eq!(work_records[0].id, id_work);

    let personal_records = svc
        .list_records(&RecordFilter::Tag("personal".into()), &sort)
        .expect("list_records must succeed");
    assert_eq!(personal_records.len(), 1);
    assert_eq!(personal_records[0].id, id_personal);

    // Non-existent tag returns empty
    let none = svc
        .list_records(&RecordFilter::Tag("nonexistent".into()), &sort)
        .expect("list_records must succeed");
    assert!(none.is_empty(), "non-existent tag should return empty");
}

// --- list_records: Expired filter returns all active records (executor filters) ---

#[test]
fn list_records_expired_returns_all_active_for_executor_filtering() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let now = Utc::now();

    // Create an expired record (expires_at in the past)
    let _id_expired = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "ExpiredSite".to_string(),
                username: "expired_user".to_string(),
                password: SecureStr::new("expired_pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: Some(now - chrono::Duration::seconds(1000)),
        })
        .expect("create_record must succeed");

    // Create a valid record (expires_at in the future)
    let _id_valid = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "ValidSite".to_string(),
                username: "valid_user".to_string(),
                password: SecureStr::new("valid_pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: Some(now + chrono::Duration::seconds(1000)),
        })
        .expect("create_record must succeed");

    // Create a record with no expiration
    let _id_no_exp = create_named_record(&mut svc, "NoExpiration");

    let sort = RecordSort {
        field: SortField::UpdatedAt,
        direction: SortDirection::Desc,
    };

    // Per spec §11.2, the vault service returns ALL active records for Expired
    // filter; the executor filters using the health report. is_expired is false
    // by default — the executor sets it from the health report.
    let records = svc
        .list_records(&RecordFilter::Expired, &sort)
        .expect("list_records must succeed");

    assert_eq!(
        records.len(),
        3,
        "vault returns all active records for Expired filter"
    );
    for record in &records {
        assert!(
            !record.is_expired,
            "is_expired defaults to false at vault service level"
        );
    }
}

// --- list_records: Trash filter returns soft-deleted records ---

#[test]
fn list_records_trash_returns_only_deleted() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let id_active = create_named_record(&mut svc, "Active");
    let id_to_delete = create_named_record(&mut svc, "ToDelete");

    svc.soft_delete_record(id_to_delete)
        .expect("soft_delete must succeed");

    let sort = RecordSort {
        field: SortField::UpdatedAt,
        direction: SortDirection::Desc,
    };

    let trash = svc
        .list_records(&RecordFilter::Trash, &sort)
        .expect("list_records must succeed");

    assert_eq!(trash.len(), 1, "only soft-deleted record in trash");
    assert_eq!(trash[0].id, id_to_delete);
    assert!(trash[0].deleted);

    // Active record should not appear in trash
    assert!(trash.iter().all(|r| r.id != id_active));
}

// --- list_records: Name sort sorts by decrypted name alphabetically ---

#[test]
fn list_records_name_sort_sorts_by_decrypted_name() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    create_named_record(&mut svc, "Charlie");
    create_named_record(&mut svc, "Alpha");
    create_named_record(&mut svc, "Bravo");

    let sort_asc = RecordSort {
        field: SortField::Name,
        direction: SortDirection::Asc,
    };

    let records = svc
        .list_records(&RecordFilter::All, &sort_asc)
        .expect("list_records must succeed");

    assert_eq!(records.len(), 3);
    assert_eq!(records[0].name, "Alpha");
    assert_eq!(records[1].name, "Bravo");
    assert_eq!(records[2].name, "Charlie");

    // Desc order
    let sort_desc = RecordSort {
        field: SortField::Name,
        direction: SortDirection::Desc,
    };

    let records_desc = svc
        .list_records(&RecordFilter::All, &sort_desc)
        .expect("list_records must succeed");

    assert_eq!(records_desc[0].name, "Charlie");
    assert_eq!(records_desc[1].name, "Bravo");
    assert_eq!(records_desc[2].name, "Alpha");
}

// --- list_records: decrypted name and subtitle are populated ---

#[test]
fn list_records_populates_decrypted_name_and_subtitle() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let _id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "GitHub".to_string(),
                username: "alice".to_string(),
                password: SecureStr::new("s3cret!".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["dev".to_string()],
            is_favorite: true,
            expires_at: None,
        })
        .expect("create_record must succeed");

    let records = svc
        .list_records(
            &RecordFilter::All,
            &RecordSort {
                field: SortField::UpdatedAt,
                direction: SortDirection::Desc,
            },
        )
        .expect("list_records must succeed");

    assert_eq!(records.len(), 1);
    let rec = &records[0];
    assert_eq!(rec.name, "GitHub", "name should be decrypted");
    assert_eq!(rec.subtitle, "alice", "subtitle should be decrypted");
    assert!(rec.is_favorite);
    assert_eq!(rec.tags, vec!["dev"]);
}

// --- list_records: Search filter returns matching records ---

#[test]
fn list_records_search_returns_matching_records() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    create_named_record(&mut svc, "Alpha");
    create_named_record(&mut svc, "Bravo");

    let result = svc.list_records(
        &RecordFilter::Search("Alpha".into()),
        &RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        },
    );

    assert!(result.is_ok());
    let records = result.unwrap();
    assert_eq!(records.len(), 1, "search should return 1 matching record");
    assert_eq!(records[0].name, "Alpha");
}

// --- list_records: Search is case-insensitive ---

#[test]
fn list_records_search_is_case_insensitive() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    create_named_record(&mut svc, "TestRecord");

    let result = svc.list_records(
        &RecordFilter::Search("testrecord".into()),
        &RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        },
    );

    assert!(result.is_ok());
    let records = result.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].name, "TestRecord");
}

// --- list_records: Search with empty query returns all active records ---

#[test]
fn list_records_search_empty_returns_all() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    create_named_record(&mut svc, "Alpha");
    create_named_record(&mut svc, "Bravo");

    let result = svc.list_records(
        &RecordFilter::Search("".into()),
        &RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        },
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 2, "empty search returns all records");
}

// --- list_records: HealthIssues returns all active records (executor filters) ---

#[test]
fn list_records_health_issues_returns_all_active_records() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    let _id_a = create_named_record(&mut svc, "Alpha");
    let _id_b = create_named_record(&mut svc, "Bravo");

    let result = svc.list_records(
        &RecordFilter::HealthIssues,
        &RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        },
    );

    assert!(result.is_ok());
    // Vault service returns all active records; executor-level filtering
    // uses the health_report to narrow down to actual health issues.
    assert_eq!(
        result.unwrap().len(),
        2,
        "HealthIssues should return all active records at vault service level"
    );
}

// =========================================================================
// decrypt_field tests
// =========================================================================

/// Helper: create an Api record and return its ID.
fn create_test_api_record(svc: &mut VaultService) -> Uuid {
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Api,
        payload: EncryptedPayload::Api {
            name: "TestApi".to_string(),
            app_id: "app-12345".to_string(),
            secret_key: SecureStr::new("sk-secret-abc".to_string()),
            url: Some("https://api.example.com".to_string()),
            notes: Some("API notes here".to_string()),
        },
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    })
    .expect("create_record must succeed")
}

/// Helper: create an Ssh record and return its ID.
fn create_test_ssh_record(svc: &mut VaultService) -> Uuid {
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Ssh,
        payload: EncryptedPayload::Ssh {
            name: "TestSsh".to_string(),
            public_key: "ssh-rsa AAAA...user@host".to_string(),
            private_key: Some(SecureStr::new(
                "-----BEGIN OPENSSH PRIVATE KEY-----\nxyz\n-----END OPENSSH PRIVATE KEY-----"
                    .to_string(),
            )),
            passphrase: None,
            notes: Some("SSH key notes".to_string()),
        },
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    })
    .expect("create_record must succeed")
}

// --- decrypt_field: NotUnlocked guard ---

#[test]
fn decrypt_field_returns_not_unlocked_when_locked() {
    let svc = setup_service();
    assert!(!svc.is_unlocked());

    let result = svc.decrypt_field(Uuid::new_v4(), FieldSelector::Password);
    assert!(result.is_err(), "decrypt_field must fail when locked");
    assert!(
        matches!(result.unwrap_err(), VaultError::NotUnlocked),
        "expected NotUnlocked error"
    );
}

// --- decrypt_field: Login record Password returns correct value ---

#[test]
fn decrypt_field_login_password_returns_correct_value() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    let value = svc
        .decrypt_field(id, FieldSelector::Password)
        .expect("decrypt_field must succeed");

    assert_eq!(value.get(), "s3cret!", "password value must match");
}

// --- decrypt_field: Login record Username returns correct value ---

#[test]
fn decrypt_field_login_username_returns_correct_value() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    let value = svc
        .decrypt_field(id, FieldSelector::Username)
        .expect("decrypt_field must succeed");

    assert_eq!(value.get(), "alice", "username value must match");
}

// --- decrypt_field: Login record Url returns correct value ---

#[test]
fn decrypt_field_login_url_returns_correct_value() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    let value = svc
        .decrypt_field(id, FieldSelector::Url)
        .expect("decrypt_field must succeed");

    assert_eq!(value.get(), "https://github.com", "url value must match");
}

// --- decrypt_field: Login record Url with None returns InvalidField ---

#[test]
fn decrypt_field_login_url_none_returns_invalid_field() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create a Login record with url = None
    let id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "NoUrl".to_string(),
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

    let result = svc.decrypt_field(id, FieldSelector::Url);
    assert!(
        matches!(
            result,
            Err(VaultError::InvalidField {
                record_type: CredentialType::Login,
                field: FieldSelector::Url
            })
        ),
        "url=None should return InvalidField, got: {:?}",
        result
    );
}

// --- decrypt_field: Api record Username returns app_id ---

#[test]
fn decrypt_field_api_username_returns_app_id() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_api_record(&mut svc);

    let value = svc
        .decrypt_field(id, FieldSelector::Username)
        .expect("decrypt_field must succeed");

    assert_eq!(value.get(), "app-12345", "Username should map to app_id");
}

// --- decrypt_field: Api record Password returns secret_key ---

#[test]
fn decrypt_field_api_password_returns_secret_key() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_api_record(&mut svc);

    let value = svc
        .decrypt_field(id, FieldSelector::Password)
        .expect("decrypt_field must succeed");

    assert_eq!(
        value.get(),
        "sk-secret-abc",
        "Password should map to secret_key"
    );
}

// --- decrypt_field: Api record Notes returns notes ---

#[test]
fn decrypt_field_api_notes_returns_notes() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_api_record(&mut svc);

    let value = svc
        .decrypt_field(id, FieldSelector::Notes)
        .expect("decrypt_field must succeed");

    assert_eq!(value.get(), "API notes here", "notes value must match");
}

// --- decrypt_field: Ssh record Url returns InvalidField ---

#[test]
fn decrypt_field_ssh_url_returns_invalid_field() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_ssh_record(&mut svc);

    let result = svc.decrypt_field(id, FieldSelector::Url);
    assert!(
        matches!(
            result,
            Err(VaultError::InvalidField {
                record_type: CredentialType::Ssh,
                field: FieldSelector::Url
            })
        ),
        "Ssh + Url should return InvalidField, got: {:?}",
        result
    );
}

// --- decrypt_field: Ssh record Password returns private_key ---

#[test]
fn decrypt_field_ssh_password_returns_private_key() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_ssh_record(&mut svc);

    let value = svc
        .decrypt_field(id, FieldSelector::Password)
        .expect("decrypt_field must succeed");

    assert!(
        value.get().contains("BEGIN OPENSSH PRIVATE KEY"),
        "Password should map to private_key"
    );
}

// --- decrypt_field: Ssh record Username returns public_key ---

#[test]
fn decrypt_field_ssh_username_returns_public_key() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_ssh_record(&mut svc);

    let value = svc
        .decrypt_field(id, FieldSelector::Username)
        .expect("decrypt_field must succeed");

    assert!(
        value.get().starts_with("ssh-rsa"),
        "Username should map to public_key"
    );
}

// --- decrypt_field: Ssh record Notes returns notes ---

#[test]
fn decrypt_field_ssh_notes_returns_notes() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_ssh_record(&mut svc);

    let value = svc
        .decrypt_field(id, FieldSelector::Notes)
        .expect("decrypt_field must succeed");

    assert_eq!(value.get(), "SSH key notes", "notes value must match");
}

// --- decrypt_field: Password field writes audit RecordViewPassword ---

#[test]
fn decrypt_field_password_writes_audit_record_view_password() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // One audit entry from create_record
    let before =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(before.len(), 1, "one audit entry from create_record");

    svc.decrypt_field(id, FieldSelector::Password)
        .expect("decrypt_field must succeed");

    let after =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(
        after.len(),
        2,
        "two audit entries after decrypt_field(Password)"
    );

    let view_entry = after
        .iter()
        .find(|e| e.operation == AuditOperation::RecordViewPassword)
        .expect("expected a RecordViewPassword audit entry");
    assert_eq!(view_entry.record_id, Some(id));
    assert_eq!(view_entry.record_name.as_deref(), Some("TestLogin"));
}

// --- decrypt_field: non-Password fields do NOT write audit ---

#[test]
fn decrypt_field_non_password_does_not_write_audit() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // One audit entry from create_record
    let before =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(before.len(), 1);

    svc.decrypt_field(id, FieldSelector::Username)
        .expect("decrypt_field must succeed");

    let after =
        queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
    assert_eq!(
        after.len(),
        1,
        "decrypt_field(Username) must not write an audit entry"
    );
}

// --- decrypt_field: nonexistent record returns RecordNotFound ---

#[test]
fn decrypt_field_returns_not_found_for_nonexistent() {
    let svc = setup_service();
    let nonexistent = Uuid::new_v4();

    let result = svc.decrypt_field(nonexistent, FieldSelector::Password);
    // NotUnlocked since service is locked
    assert!(result.is_err());
}

// --- decrypt_field: nonexistent record returns RecordNotFound when unlocked ---

#[test]
fn decrypt_field_returns_not_found_for_nonexistent_when_unlocked() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let nonexistent = Uuid::new_v4();

    let result = svc.decrypt_field(nonexistent, FieldSelector::Password);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
        "expected RecordNotFound"
    );
}

// =========================================================================
// Health state lifecycle tests (Task F)
// =========================================================================

use crate::types::health::RecordHealthState;

/// Helper: insert a health state row for a record at a given version.
fn insert_health_state(svc: &VaultService, record_id: Uuid, version: u64) {
    let state = RecordHealthState {
        record_id,
        record_version: version,
        evaluated_at: Some(Utc::now()),
        weak_password: Some(true),
        duplicate_group_size: None,
        compromised: Some(false),
        expired: Some(false),
    };
    svc.upsert_record_health_state(&state)
        .expect("upsert_record_health_state must succeed");
}

/// Helper: assert that no health state row exists for the given record.
fn assert_no_health_state(svc: &VaultService, record_id: Uuid) {
    let state = queries::get_record_health_state(&svc.conn, &record_id)
        .expect("get_record_health_state must succeed");
    assert!(state.is_none(), "expected no health state for record");
}

/// Helper: assert that a health state row exists at the given version.
fn assert_health_state_version(svc: &VaultService, record_id: Uuid, expected_version: u64) {
    let state = queries::get_record_health_state(&svc.conn, &record_id)
        .expect("get_record_health_state must succeed")
        .expect("health state must exist");
    assert_eq!(
        state.record_version, expected_version,
        "health state version mismatch"
    );
}

// --- update_record: password change deletes health state ---

#[test]
fn update_record_password_change_deletes_health_state() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Insert health state at version 1
    insert_health_state(&svc, id, 1);
    assert_health_state_version(&svc, id, 1);

    // Update with a different password
    let new_payload = EncryptedPayload::Login {
        name: "TestLogin".to_string(),
        username: "alice".to_string(),
        password: SecureStr::new("newP@ssw0rd!".to_string()),
        url: Some("https://github.com".to_string()),
        notes: None,
    };

    svc.update_record(UpdateRecordParams {
        id,
        payload: new_payload,
        tags: vec![],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    })
    .expect("update_record must succeed");

    // Health state should be deleted
    assert_no_health_state(&svc, id);
}

// --- update_record: expires_at change deletes health state ---

#[test]
fn update_record_expires_at_change_deletes_health_state() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Insert health state at version 1
    insert_health_state(&svc, id, 1);

    // Update with a new expires_at (same password)
    let now = Utc::now();
    let new_payload = EncryptedPayload::Login {
        name: "TestLogin".to_string(),
        username: "alice".to_string(),
        password: SecureStr::new("s3cret!".to_string()),
        url: Some("https://github.com".to_string()),
        notes: None,
    };

    svc.update_record(UpdateRecordParams {
        id,
        payload: new_payload,
        tags: vec![],
        is_favorite: false,
        expires_at: Some(now + chrono::Duration::days(30)),
        expected_version: 1,
    })
    .expect("update_record must succeed");

    // Health state should be deleted
    assert_no_health_state(&svc, id);
}

// --- update_record: cosmetic change carries health state forward ---

#[test]
fn update_record_cosmetic_change_carries_health_state_version() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Insert health state at version 1
    insert_health_state(&svc, id, 1);

    // Update only name (same password, same expires_at)
    let new_payload = EncryptedPayload::Login {
        name: "TestLoginRenamed".to_string(),
        username: "alice".to_string(),
        password: SecureStr::new("s3cret!".to_string()),
        url: Some("https://github.com".to_string()),
        notes: None,
    };

    svc.update_record(UpdateRecordParams {
        id,
        payload: new_payload,
        tags: vec![],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    })
    .expect("update_record must succeed");

    // Health state should still exist at version 2
    assert_health_state_version(&svc, id, 2);
}

// --- update_record: cosmetic change preserves health state flags ---

#[test]
fn update_record_cosmetic_change_preserves_health_flags() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Insert health state with specific flags at version 1
    let state = RecordHealthState {
        record_id: id,
        record_version: 1,
        evaluated_at: Some(Utc::now()),
        weak_password: Some(true),
        duplicate_group_size: Some(3),
        compromised: Some(false),
        expired: Some(true),
    };
    svc.upsert_record_health_state(&state)
        .expect("upsert must succeed");

    // Update only tags (cosmetic change)
    let new_payload = EncryptedPayload::Login {
        name: "TestLogin".to_string(),
        username: "alice".to_string(),
        password: SecureStr::new("s3cret!".to_string()),
        url: Some("https://github.com".to_string()),
        notes: None,
    };

    svc.update_record(UpdateRecordParams {
        id,
        payload: new_payload,
        tags: vec!["new-tag".to_string()],
        is_favorite: true,
        expires_at: None,
        expected_version: 1,
    })
    .expect("update_record must succeed");

    // Health state should have version 2 and same flags
    let after = queries::get_record_health_state(&svc.conn, &id)
        .expect("query must succeed")
        .expect("health state must exist");
    assert_eq!(after.record_version, 2);
    assert_eq!(after.weak_password, Some(true));
    assert_eq!(after.duplicate_group_size, Some(3));
    assert_eq!(after.compromised, Some(false));
    assert_eq!(after.expired, Some(true));
}

// --- update_record: no prior health state, cosmetic change is no-op ---

#[test]
fn update_record_cosmetic_change_without_prior_health_state_is_noop() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // No health state inserted — update should succeed without error
    let new_payload = EncryptedPayload::Login {
        name: "TestLoginRenamed".to_string(),
        username: "alice".to_string(),
        password: SecureStr::new("s3cret!".to_string()),
        url: Some("https://github.com".to_string()),
        notes: None,
    };

    svc.update_record(UpdateRecordParams {
        id,
        payload: new_payload,
        tags: vec![],
        is_favorite: false,
        expires_at: None,
        expected_version: 1,
    })
    .expect("update_record must succeed");

    // No health state should exist (copy to version is a no-op)
    assert_no_health_state(&svc, id);
}

// --- soft_delete_record: deletes health state ---

#[test]
fn soft_delete_record_deletes_health_state() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Insert health state
    insert_health_state(&svc, id, 1);
    assert_health_state_version(&svc, id, 1);

    svc.soft_delete_record(id)
        .expect("soft_delete_record must succeed");

    // Health state should be deleted
    assert_no_health_state(&svc, id);
}

// --- hard_delete_record: deletes health state ---

#[test]
fn hard_delete_record_deletes_health_state() {
    let mut svc = setup_service();
    unlock_service(&mut svc);
    let id = create_test_login_record(&mut svc);

    // Insert health state
    insert_health_state(&svc, id, 1);

    svc.hard_delete_record(id)
        .expect("hard_delete_record must succeed");

    // Health state should be gone
    assert_no_health_state(&svc, id);
}

// --- update_record: expires_at changing to None deletes health state ---

#[test]
fn update_record_expires_at_removed_deletes_health_state() {
    let mut svc = setup_service();
    unlock_service(&mut svc);

    // Create record with expires_at set
    let now = Utc::now();
    let id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Expiring".to_string(),
                username: "alice".to_string(),
                password: SecureStr::new("s3cret!".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: Some(now + chrono::Duration::days(30)),
        })
        .expect("create_record must succeed");

    // Insert health state
    insert_health_state(&svc, id, 1);

    // Update: remove expires_at (same password)
    let new_payload = EncryptedPayload::Login {
        name: "Expiring".to_string(),
        username: "alice".to_string(),
        password: SecureStr::new("s3cret!".to_string()),
        url: None,
        notes: None,
    };

    svc.update_record(UpdateRecordParams {
        id,
        payload: new_payload,
        tags: vec![],
        is_favorite: false,
        expires_at: None, // Changed from Some to None
        expected_version: 1,
    })
    .expect("update_record must succeed");

    // Health state should be deleted
    assert_no_health_state(&svc, id);
}
