//! Integration tests for S3 Health Service.
//!
//! HIBP tests that require network are NOT marked #[ignore] but handle
//! network failure gracefully — they skip instead of failing in CI.

use chrono::Utc;
use oak_keyring::commands::types::{HealthIssue, HealthReport};
use oak_keyring::config::security::{HealthCheckFrequency, SecurityConfig};
use oak_keyring::crypto::strength::evaluate_strength;
use oak_keyring::services::health::{should_run, FnDecryptor, Health, HealthService};
use oak_keyring::types::credential::CredentialType;
use oak_keyring::types::record::StoredRecord;
use oak_keyring::types::sensitive::SecureStr;
use uuid::Uuid;

fn make_rec(id: Uuid) -> StoredRecord {
    StoredRecord {
        id,
        credential_type: CredentialType::Login,
        encrypted_data: vec![],
        nonce: [0u8; 24],
        dek_version: 1,
        aad: vec![],
        is_favorite: false,
        expires_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        updated_by: "test".into(),
        version: 1,
        deleted: false,
        deleted_at: None,
        tags: vec![],
    }
}

#[test]
fn acceptance_weak_password_123_detected() {
    // AC#1: password "123" detected as VeryWeak
    let strength = evaluate_strength("123");
    assert!(
        matches!(
            strength.level,
            oak_keyring::crypto::strength::StrengthLevel::VeryWeak
        ),
        "expected VeryWeak, got {:?}",
        strength.level
    );
}

#[test]
fn acceptance_duplicate_password_grouping() {
    // AC#2: two records with same password -> duplicate group
    let service = HealthService::new();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    let records = vec![make_rec(id1), make_rec(id2)];
    let decrypt_fn = move |_| Ok(SecureStr::new("abc123".to_string()));

    let report = service.run_full_check(&records, &FnDecryptor(decrypt_fn));
    assert_eq!(report.duplicate_passwords.len(), 1);
    assert_eq!(report.duplicate_passwords[0].len(), 2);
}

#[test]
fn acceptance_hibp_check_password_is_compromised() {
    // AC#3: "password" is known to be compromised in HIBP
    // Gracefully handles network unavailability (CI)
    let service = HealthService::new();
    let result = service.check_hibp_single(&SecureStr::new("password".to_string()));
    match result {
        Ok(compromised) => assert!(compromised, "expected 'password' to be compromised"),
        Err(e) => {
            // Network unavailable in CI -- skip, don't fail
            eprintln!("HIBP test skipped: {}", e);
        }
    }
}

#[test]
fn acceptance_get_issue_for_priority_ordering() {
    // AC#5: Weak beats Duplicate
    let id = Uuid::new_v4();
    let report = HealthReport {
        weak_passwords: vec![id],
        duplicate_passwords: vec![vec![id, Uuid::new_v4()]],
        ..HealthReport::empty()
    };
    assert_eq!(report.get_issue_for(id), Some(HealthIssue::Weak));

    // AC#6: Compromised beats everything
    let report = HealthReport {
        weak_passwords: vec![id],
        compromised: vec![id],
        ..HealthReport::empty()
    };
    assert_eq!(report.get_issue_for(id), Some(HealthIssue::Compromised));
}

#[test]
fn acceptance_should_run_disabled() {
    // AC#7: disabled returns false
    let config = SecurityConfig {
        health_check_enabled: false,
        ..Default::default()
    };
    assert!(!should_run(&config, None));
}

#[test]
fn acceptance_should_run_daily_within_24h() {
    // AC#8: Daily, checked 23h ago -> false
    let config = SecurityConfig {
        health_check_enabled: true,
        health_check_frequency: HealthCheckFrequency::Daily,
        ..Default::default()
    };
    let recent = chrono::Utc::now() - chrono::Duration::hours(23);
    assert!(!should_run(&config, Some(recent)));
}

#[test]
fn acceptance_expired_yesterday_detected() {
    // AC#9: expires_at = yesterday -> in expired list
    let past = chrono::Utc::now() - chrono::Duration::days(1);
    let mut rec = make_rec(Uuid::new_v4());
    rec.expires_at = Some(past);

    let service = HealthService::new();
    let report = service.run_full_check(
        &[rec],
        &FnDecryptor(|_| Ok(SecureStr::new("strongpass".to_string()))),
    );
    assert!(!report.expired.is_empty());
}

#[test]
fn acceptance_expired_tomorrow_not_detected() {
    // AC#10: expires_at = tomorrow -> NOT in expired list
    let future = chrono::Utc::now() + chrono::Duration::days(1);
    let mut rec = make_rec(Uuid::new_v4());
    rec.expires_at = Some(future);

    let service = HealthService::new();
    let report = service.run_full_check(
        &[rec],
        &FnDecryptor(|_| Ok(SecureStr::new("strongpass".to_string()))),
    );
    assert!(report.expired.is_empty());
}

// =========================================================================
// Task J: Health check state lifecycle integration tests
// =========================================================================

use oak_keyring::db::schema::init_db_in_memory;
use oak_keyring::services::vault::VaultService;
use oak_keyring::types::health::RecordHealthState;

fn setup_vault() -> VaultService {
    let conn = init_db_in_memory();
    VaultService::new(conn)
}

/// Helper: create a record and return its ID.
fn create_record(vault: &mut VaultService, name: &str) -> uuid::Uuid {
    use oak_keyring::crypto::bip39::{MnemonicLanguage, Passkey};
    use oak_keyring::types::{EncryptedPayload, SecureStr};

    if !vault.is_unlocked() {
        let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
        vault.unlock_with_mnemonic(&mnemonic).expect("unlock");
    }

    vault
        .create_record(oak_keyring::types::record::CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: name.to_string(),
                username: format!("user_{}", name),
                password: SecureStr::new("password123".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create record")
}

/// Helper: insert a health state.
fn insert_state(vault: &VaultService, record_id: uuid::Uuid, weak: bool, compromised: bool) {
    vault
        .upsert_record_health_state(&RecordHealthState {
            record_id,
            record_version: 1,
            evaluated_at: Some(Utc::now()),
            weak_password: Some(weak),
            duplicate_group_size: None,
            compromised: Some(compromised),
            expired: Some(false),
        })
        .expect("upsert health state");
}

/// Helper: find a health state by record ID from the list.
fn find_state(vault: &VaultService, record_id: uuid::Uuid) -> Option<RecordHealthState> {
    vault
        .list_record_health_states()
        .expect("list")
        .into_iter()
        .find(|s| s.record_id == record_id)
}

#[test]
fn integration_health_state_round_trip_crud() {
    // Verify the full health state lifecycle: insert -> read -> update -> delete
    let mut vault = setup_vault();
    let id = create_record(&mut vault, "crud_test");

    // Insert
    insert_state(&vault, id, true, false);

    // Read via list
    let state = find_state(&vault, id).expect("state should exist");
    assert_eq!(state.weak_password, Some(true));
    assert_eq!(state.compromised, Some(false));

    // Update (upsert)
    insert_state(&vault, id, false, true);
    let updated = find_state(&vault, id).expect("state should exist");
    assert_eq!(updated.weak_password, Some(false));
    assert_eq!(updated.compromised, Some(true));

    // Delete via VaultService
    vault.delete_record_health_state(&id).expect("delete");
    assert!(
        find_state(&vault, id).is_none(),
        "health state should be deleted"
    );
}

#[test]
fn integration_password_change_clears_health_state() {
    // When password changes, health state should be cleared (via update_record)
    let mut vault = setup_vault();
    let id = create_record(&mut vault, "pw_change");

    // Insert health state
    insert_state(&vault, id, true, false);

    // Update record with a different password
    use oak_keyring::types::{EncryptedPayload, SecureStr};
    vault
        .update_record(oak_keyring::types::record::UpdateRecordParams {
            id,
            payload: EncryptedPayload::Login {
                name: "pw_change".to_string(),
                username: "user_pw_change".to_string(),
                password: SecureStr::new("completely_new_password!".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        })
        .expect("update");

    // Health state should be cleared
    assert!(
        find_state(&vault, id).is_none(),
        "password change should clear health state"
    );
}

#[test]
fn integration_cosmetic_update_preserves_health_state() {
    // When only cosmetic fields change (notes, tags), health state is preserved
    let mut vault = setup_vault();
    let id = create_record(&mut vault, "cosmetic");

    // Insert health state
    insert_state(&vault, id, true, false);

    // Update record with same password (only tags change)
    use oak_keyring::types::{EncryptedPayload, SecureStr};
    vault
        .update_record(oak_keyring::types::record::UpdateRecordParams {
            id,
            payload: EncryptedPayload::Login {
                name: "cosmetic_renamed".to_string(),
                username: "user_cosmetic".to_string(),
                password: SecureStr::new("password123".to_string()), // same password
                url: None,
                notes: Some("new notes".to_string()),
            },
            tags: vec!["new-tag".to_string()],
            is_favorite: true,
            expires_at: None,
            expected_version: 1,
        })
        .expect("update");

    // Health state should be preserved with updated version
    let state = find_state(&vault, id).expect("state should exist");
    assert_eq!(state.weak_password, Some(true), "flags should be preserved");
    assert_eq!(state.compromised, Some(false));
    assert_eq!(state.record_version, 2, "version should advance");
}

#[test]
fn integration_soft_delete_cleans_up_health_state() {
    // Soft-deleting a record should cascade to health state
    let mut vault = setup_vault();
    let id = create_record(&mut vault, "to_soft_delete");

    insert_state(&vault, id, true, false);

    // Verify health state exists
    assert!(find_state(&vault, id).is_some());

    // Soft delete
    vault.soft_delete_record(id).expect("soft delete");

    // Health state should be gone
    assert!(
        find_state(&vault, id).is_none(),
        "soft delete should clean up health state"
    );
}

#[test]
fn integration_hard_delete_cleans_up_health_state() {
    // Hard-deleting a record should cascade to health state
    let mut vault = setup_vault();
    let id = create_record(&mut vault, "to_hard_delete");

    insert_state(&vault, id, true, false);

    // Hard delete
    vault.hard_delete_record(id).expect("hard delete");

    // Health state should be gone
    assert!(
        find_state(&vault, id).is_none(),
        "hard delete should clean up health state"
    );
}

#[test]
fn integration_metadata_timestamp_round_trip_via_vault_service() {
    // Verify last_health_check_at persists through vault service
    let mut vault = setup_vault();

    // Initially no timestamp
    assert!(vault.get_last_health_check_at().expect("get").is_none());

    // Set timestamp
    let ts = Utc::now();
    vault.set_last_health_check_at(ts).expect("set");

    // Read back
    let stored = vault
        .get_last_health_check_at()
        .expect("get")
        .expect("should exist");
    let diff = (stored - ts).num_seconds().abs();
    assert!(
        diff <= 1,
        "round-tripped timestamp should be within 1s, got diff={diff}"
    );

    // Verify corrupted value is handled gracefully
    vault
        .set_metadata("last_health_check_at", "not-a-timestamp")
        .expect("set garbage");
    assert!(
        vault.get_last_health_check_at().expect("get").is_none(),
        "corrupted value should return None"
    );
}

#[test]
fn integration_should_run_all_frequencies() {
    // Verify all frequency modes of should_run
    let disabled_config = SecurityConfig {
        health_check_enabled: false,
        ..Default::default()
    };

    // Disabled always returns false
    assert!(!should_run(&disabled_config, None));
    assert!(!should_run(
        &disabled_config,
        Some(Utc::now() - chrono::Duration::days(30))
    ));

    // OnStartup always returns true when enabled
    let startup_config = SecurityConfig {
        health_check_enabled: true,
        health_check_frequency: HealthCheckFrequency::OnStartup,
        ..Default::default()
    };
    assert!(should_run(&startup_config, None));
    assert!(should_run(&startup_config, Some(Utc::now())));

    // Daily: should run after 24h+
    let daily_config = SecurityConfig {
        health_check_enabled: true,
        health_check_frequency: HealthCheckFrequency::Daily,
        ..Default::default()
    };
    assert!(should_run(&daily_config, None), "never checked -> run");
    assert!(
        !should_run(
            &daily_config,
            Some(Utc::now() - chrono::Duration::hours(23))
        ),
        "23h ago -> skip"
    );
    assert!(
        should_run(
            &daily_config,
            Some(Utc::now() - chrono::Duration::hours(25))
        ),
        "25h ago -> run"
    );

    // Weekly: should run after 7d+
    let weekly_config = SecurityConfig {
        health_check_enabled: true,
        health_check_frequency: HealthCheckFrequency::Weekly,
        ..Default::default()
    };
    assert!(should_run(&weekly_config, None), "never checked -> run");
    assert!(
        !should_run(&weekly_config, Some(Utc::now() - chrono::Duration::days(6))),
        "6d ago -> skip"
    );
    assert!(
        should_run(&weekly_config, Some(Utc::now() - chrono::Duration::days(8))),
        "8d ago -> run"
    );
}
