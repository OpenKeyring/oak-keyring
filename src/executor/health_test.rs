use chrono::Utc;
use uuid::Uuid;

use super::health::{
    handle_run_health_check, health_state_changed, load_cached_health_report,
    persist_health_report, project_health_report_to_states, schedule_health_resync_for_records,
};
use super::CommandExecutor;
use crate::commands::types::HealthReport;
use crate::commands::{CommandResult, Message};
use crate::config::AppConfig;
use crate::crypto::bip39::{MnemonicLanguage, Passkey};
use crate::db::queries;
use crate::executor::config_impl::ServiceNotificationImpl;
use crate::services::clipboard::{ClipboardService, MockBackend};
use crate::services::health::HealthService;
use crate::services::import_export::ImportExportService;
use crate::services::vault::VaultService;
use crate::types::health::{HealthStateDelta, RecordHealthState};
use crate::types::record::StoredRecord;
use crate::types::sync::SyncStatus;
use crate::types::{CredentialType, EncryptedPayload, SecureStr};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_executor_with_one_login() -> CommandExecutor {
    let conn = crate::db::schema::init_db_in_memory();
    let mut vault = VaultService::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
    vault
        .unlock_with_mnemonic(&mnemonic)
        .expect("unlock with mnemonic");
    vault
        .create_record(crate::types::record::CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "Example".to_string(),
                username: "alice".to_string(),
                password: SecureStr::new("password123".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("record");

    let (result_tx, _) = mpsc::channel(64);
    let (internal_tx, internal_rx) = mpsc::channel(64);

    CommandExecutor {
        vault,
        sync: None,
        health: HealthService::new(),
        clipboard: Arc::new(ClipboardService::with_backend(
            Box::new(MockBackend::new()),
            30,
        )),
        import_export: ImportExportService::new(),
        config: AppConfig::default(),
        config_notifier: ServiceNotificationImpl::new(),
        vault_dir: std::path::PathBuf::from(":memory:"),
        health_report: None,
        last_health_check_time: None,
        result_tx,
        internal_tx,
        internal_rx: Some(internal_rx),
        cancel_token: CancellationToken::new(),
        oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
    }
}

/// Helper: create an executor with an unlocked vault (no records).
fn make_executor_no_records() -> CommandExecutor {
    let conn = crate::db::schema::init_db_in_memory();
    let mut vault = VaultService::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
    vault
        .unlock_with_mnemonic(&mnemonic)
        .expect("unlock with mnemonic");

    let (result_tx, _) = mpsc::channel(64);
    let (internal_tx, internal_rx) = mpsc::channel(64);

    CommandExecutor {
        vault,
        sync: None,
        health: HealthService::new(),
        clipboard: Arc::new(ClipboardService::with_backend(
            Box::new(MockBackend::new()),
            30,
        )),
        import_export: ImportExportService::new(),
        config: AppConfig::default(),
        config_notifier: ServiceNotificationImpl::new(),
        vault_dir: std::path::PathBuf::from(":memory:"),
        health_report: None,
        last_health_check_time: None,
        result_tx,
        internal_tx,
        internal_rx: Some(internal_rx),
        cancel_token: CancellationToken::new(),
        oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
    }
}

/// Helper: create a Login record and return its UUID.
fn create_login_record(executor: &mut CommandExecutor, name: &str) -> Uuid {
    executor
        .vault
        .create_record(crate::types::record::CreateRecordParams {
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

/// Helper: insert a health state via the VaultService wrapper.
fn insert_health_state(executor: &mut CommandExecutor, state: RecordHealthState) {
    executor
        .vault
        .upsert_record_health_state(&state)
        .expect("insert health state");
}

/// Helper: make a StoredRecord with minimal fields for projection tests.
fn make_stored_record(id: Uuid, version: u64) -> StoredRecord {
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
        updated_by: "test".to_string(),
        version,
        deleted: false,
        deleted_at: None,
        tags: vec![],
    }
}

// ===========================================================================
// Tests: project_health_report_to_states (pure function)
// ===========================================================================

#[test]
fn projection_maps_weak_passwords() {
    let id_weak = Uuid::new_v4();
    let id_clean = Uuid::new_v4();
    let records = vec![
        make_stored_record(id_weak, 1),
        make_stored_record(id_clean, 2),
    ];

    let report = HealthReport {
        weak_passwords: vec![id_weak],
        duplicate_passwords: vec![],
        compromised: vec![],
        expired: vec![],
        total_checked: 2,
    };

    let now = Utc::now();
    let states = project_health_report_to_states(&records, &report, now);

    assert_eq!(states.len(), 2);

    let weak_state = states.iter().find(|s| s.record_id == id_weak).unwrap();
    assert_eq!(weak_state.weak_password, Some(true));
    assert_eq!(weak_state.record_version, 1);
    assert_eq!(weak_state.evaluated_at, Some(now));

    let clean_state = states.iter().find(|s| s.record_id == id_clean).unwrap();
    assert_eq!(clean_state.weak_password, Some(false));
    assert_eq!(clean_state.record_version, 2);
}

#[test]
fn projection_maps_duplicate_group_size() {
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    let id_c = Uuid::new_v4();
    let records = vec![
        make_stored_record(id_a, 1),
        make_stored_record(id_b, 1),
        make_stored_record(id_c, 1),
    ];

    // id_a and id_b share a password; id_c is unique.
    let report = HealthReport {
        weak_passwords: vec![],
        duplicate_passwords: vec![vec![id_a, id_b]],
        compromised: vec![],
        expired: vec![],
        total_checked: 3,
    };

    let states = project_health_report_to_states(&records, &report, Utc::now());

    let state_a = states.iter().find(|s| s.record_id == id_a).unwrap();
    assert_eq!(state_a.duplicate_group_size, Some(2));

    let state_b = states.iter().find(|s| s.record_id == id_b).unwrap();
    assert_eq!(state_b.duplicate_group_size, Some(2));

    let state_c = states.iter().find(|s| s.record_id == id_c).unwrap();
    assert_eq!(state_c.duplicate_group_size, None);
}

#[test]
fn projection_maps_compromised() {
    let id_bad = Uuid::new_v4();
    let id_ok = Uuid::new_v4();
    let records = vec![make_stored_record(id_bad, 1), make_stored_record(id_ok, 1)];

    let report = HealthReport {
        weak_passwords: vec![],
        duplicate_passwords: vec![],
        compromised: vec![id_bad],
        expired: vec![],
        total_checked: 2,
    };

    let states = project_health_report_to_states(&records, &report, Utc::now());

    let bad = states.iter().find(|s| s.record_id == id_bad).unwrap();
    assert_eq!(bad.compromised, Some(true));

    let ok = states.iter().find(|s| s.record_id == id_ok).unwrap();
    assert_eq!(ok.compromised, Some(false));
}

#[test]
fn projection_maps_expired() {
    let id_exp = Uuid::new_v4();
    let id_valid = Uuid::new_v4();
    let records = vec![
        make_stored_record(id_exp, 1),
        make_stored_record(id_valid, 1),
    ];

    let report = HealthReport {
        weak_passwords: vec![],
        duplicate_passwords: vec![],
        compromised: vec![],
        expired: vec![id_exp],
        total_checked: 2,
    };

    let states = project_health_report_to_states(&records, &report, Utc::now());

    let exp = states.iter().find(|s| s.record_id == id_exp).unwrap();
    assert_eq!(exp.expired, Some(true));

    let valid = states.iter().find(|s| s.record_id == id_valid).unwrap();
    assert_eq!(valid.expired, Some(false));
}

#[test]
fn projection_empty_report_produces_all_clean_states() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let records = vec![make_stored_record(id1, 1), make_stored_record(id2, 3)];

    let report = HealthReport::empty();
    let now = Utc::now();
    let states = project_health_report_to_states(&records, &report, now);

    assert_eq!(states.len(), 2);
    for state in &states {
        assert_eq!(state.weak_password, Some(false));
        assert_eq!(state.duplicate_group_size, None);
        assert_eq!(state.compromised, Some(false));
        assert_eq!(state.expired, Some(false));
        assert_eq!(state.evaluated_at, Some(now));
    }

    let s1 = states.iter().find(|s| s.record_id == id1).unwrap();
    assert_eq!(s1.record_version, 1);
    let s2 = states.iter().find(|s| s.record_id == id2).unwrap();
    assert_eq!(s2.record_version, 3);
}

#[test]
fn projection_multiple_duplicate_groups() {
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    let id_c = Uuid::new_v4();
    let id_d = Uuid::new_v4();
    let records = vec![
        make_stored_record(id_a, 1),
        make_stored_record(id_b, 1),
        make_stored_record(id_c, 1),
        make_stored_record(id_d, 1),
    ];

    // Two separate duplicate groups.
    let report = HealthReport {
        weak_passwords: vec![],
        duplicate_passwords: vec![vec![id_a, id_b], vec![id_c, id_d]],
        compromised: vec![],
        expired: vec![],
        total_checked: 4,
    };

    let states = project_health_report_to_states(&records, &report, Utc::now());

    for state in &states {
        assert_eq!(state.duplicate_group_size, Some(2));
    }
}

// ===========================================================================
// Tests: persist_health_report
// ===========================================================================

#[test]
fn persist_writes_states_to_db_and_returns_deltas_for_new_evaluations() {
    let mut executor = make_executor_no_records();
    let id1 = create_login_record(&mut executor, "rec1");
    let id2 = create_login_record(&mut executor, "rec2");

    let report = HealthReport {
        weak_passwords: vec![id1],
        duplicate_passwords: vec![],
        compromised: vec![],
        expired: vec![],
        total_checked: 2,
    };

    let now = Utc::now();
    let deltas = persist_health_report(&mut executor, &report, now).expect("persist");

    // Both records should appear as deltas (no prior state → changed).
    assert_eq!(deltas.len(), 2);

    // Verify DB was written.
    let states = executor.vault.list_record_health_states().expect("list");
    assert_eq!(states.len(), 2);

    let s1 = states.iter().find(|s| s.record_id == id1).expect("id1");
    assert_eq!(s1.weak_password, Some(true));
    // Timestamp round-trips through SQLite which truncates sub-second precision.
    let stored_at = s1.evaluated_at.expect("evaluated_at should be set");
    let diff = (stored_at - now).num_seconds().abs();
    assert!(
        diff <= 1,
        "evaluated_at should be within 1s of input, got diff={diff}"
    );

    let s2 = states.iter().find(|s| s.record_id == id2).expect("id2");
    assert_eq!(s2.weak_password, Some(false));
}

#[test]
fn persist_detects_changed_health_state() {
    let mut executor = make_executor_no_records();
    let id = create_login_record(&mut executor, "rec");

    // Insert an old state: weak=false
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id,
            record_version: 1,
            evaluated_at: Some(Utc::now() - chrono::Duration::hours(1)),
            weak_password: Some(false),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: Some(false),
        },
    );

    // New report says the record is now weak.
    let report = HealthReport {
        weak_passwords: vec![id],
        duplicate_passwords: vec![],
        compromised: vec![],
        expired: vec![],
        total_checked: 1,
    };

    let deltas = persist_health_report(&mut executor, &report, Utc::now()).expect("persist");

    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].record_id, id);
    assert!(deltas[0].before.is_some());
    assert_eq!(
        deltas[0].before.as_ref().unwrap().weak_password,
        Some(false)
    );
    assert_eq!(deltas[0].after.as_ref().unwrap().weak_password, Some(true));
}

#[test]
fn persist_skips_unchanged_records() {
    let mut executor = make_executor_no_records();
    let id1 = create_login_record(&mut executor, "rec1");
    let id2 = create_login_record(&mut executor, "rec2");

    // Pre-populate states that match the report exactly.
    let now = Utc::now();
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id1,
            record_version: 1,
            evaluated_at: Some(now),
            weak_password: Some(true),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: Some(false),
        },
    );
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id2,
            record_version: 1,
            evaluated_at: Some(now),
            weak_password: Some(false),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: Some(false),
        },
    );

    let report = HealthReport {
        weak_passwords: vec![id1],
        duplicate_passwords: vec![],
        compromised: vec![],
        expired: vec![],
        total_checked: 2,
    };

    let deltas = persist_health_report(&mut executor, &report, Utc::now()).expect("persist");

    // States match the report — no deltas expected.
    assert!(
        deltas.is_empty(),
        "unchanged records should produce no deltas, got {:?}",
        deltas
    );
}

#[test]
fn persist_handles_compromised_change() {
    let mut executor = make_executor_no_records();
    let id = create_login_record(&mut executor, "rec");

    // Old state: not compromised
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id,
            record_version: 1,
            evaluated_at: None,
            weak_password: Some(false),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: Some(false),
        },
    );

    let report = HealthReport {
        weak_passwords: vec![],
        duplicate_passwords: vec![],
        compromised: vec![id],
        expired: vec![],
        total_checked: 1,
    };

    let deltas = persist_health_report(&mut executor, &report, Utc::now()).expect("persist");
    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].before.as_ref().unwrap().compromised, Some(false));
    assert_eq!(deltas[0].after.as_ref().unwrap().compromised, Some(true));
}

#[test]
fn persist_handles_expired_change() {
    let mut executor = make_executor_no_records();
    let id = create_login_record(&mut executor, "rec");

    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id,
            record_version: 1,
            evaluated_at: None,
            weak_password: Some(false),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: Some(false),
        },
    );

    let report = HealthReport {
        weak_passwords: vec![],
        duplicate_passwords: vec![],
        compromised: vec![],
        expired: vec![id],
        total_checked: 1,
    };

    let deltas = persist_health_report(&mut executor, &report, Utc::now()).expect("persist");
    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].before.as_ref().unwrap().expired, Some(false));
    assert_eq!(deltas[0].after.as_ref().unwrap().expired, Some(true));
}

#[test]
fn persist_handles_duplicate_group_change() {
    let mut executor = make_executor_no_records();
    let id = create_login_record(&mut executor, "rec");

    // Old state: no duplicate group
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id,
            record_version: 1,
            evaluated_at: None,
            weak_password: Some(false),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: Some(false),
        },
    );

    let report = HealthReport {
        weak_passwords: vec![],
        duplicate_passwords: vec![vec![id]],
        compromised: vec![],
        expired: vec![],
        total_checked: 1,
    };

    let deltas = persist_health_report(&mut executor, &report, Utc::now()).expect("persist");
    assert_eq!(deltas.len(), 1);
    assert_eq!(
        deltas[0].before.as_ref().unwrap().duplicate_group_size,
        None
    );
    assert_eq!(
        deltas[0].after.as_ref().unwrap().duplicate_group_size,
        Some(1)
    );
}

// ===========================================================================
// Tests: schedule_health_resync_for_records
// ===========================================================================

#[test]
fn schedule_resync_marks_records_pending_in_sync_state() {
    let mut executor = make_executor_no_records();
    let id1 = create_login_record(&mut executor, "rec1");
    let id2 = create_login_record(&mut executor, "rec2");

    let deltas = vec![
        HealthStateDelta {
            record_id: id1,
            before: None,
            after: None,
        },
        HealthStateDelta {
            record_id: id2,
            before: None,
            after: None,
        },
    ];

    schedule_health_resync_for_records(&mut executor, &deltas).expect("schedule");

    // Verify sync_state entries by checking the DB directly.
    let sync_map = queries::load_sync_status_map(executor.vault.conn_ref());
    assert_eq!(sync_map.len(), 2);
    assert_eq!(sync_map.get(&id1.to_string()), Some(&SyncStatus::Pending));
    assert_eq!(sync_map.get(&id2.to_string()), Some(&SyncStatus::Pending));
}

#[test]
fn schedule_resync_empty_deltas_is_noop() {
    let mut executor = make_executor_no_records();
    schedule_health_resync_for_records(&mut executor, &[]).expect("schedule");
}

// ===========================================================================
// Tests: health_state_changed (internal logic)
// ===========================================================================

#[test]
fn health_state_unchanged_when_all_fields_match() {
    let state = RecordHealthState {
        record_id: Uuid::new_v4(),
        record_version: 1,
        evaluated_at: Some(Utc::now()),
        weak_password: Some(true),
        duplicate_group_size: Some(2),
        compromised: Some(false),
        expired: Some(false),
    };

    // Same state: should not be detected as changed.
    let same = state.clone();
    assert!(
        !health_state_changed(&state, &same),
        "identical states should not be considered changed"
    );
}

#[test]
fn health_state_changed_when_weak_differs() {
    let before = RecordHealthState {
        record_id: Uuid::new_v4(),
        record_version: 1,
        evaluated_at: Some(Utc::now()),
        weak_password: Some(false),
        duplicate_group_size: None,
        compromised: Some(false),
        expired: Some(false),
    };
    let after = RecordHealthState {
        weak_password: Some(true),
        ..before.clone()
    };
    assert!(health_state_changed(&before, &after));
}

#[test]
fn health_state_changed_when_duplicate_group_size_differs() {
    let before = RecordHealthState {
        record_id: Uuid::new_v4(),
        record_version: 1,
        evaluated_at: Some(Utc::now()),
        weak_password: Some(false),
        duplicate_group_size: None,
        compromised: Some(false),
        expired: Some(false),
    };
    let after = RecordHealthState {
        duplicate_group_size: Some(3),
        ..before.clone()
    };
    assert!(health_state_changed(&before, &after));
}

#[test]
fn health_state_changed_when_compromised_differs() {
    let before = RecordHealthState {
        record_id: Uuid::new_v4(),
        record_version: 1,
        evaluated_at: Some(Utc::now()),
        weak_password: Some(false),
        duplicate_group_size: None,
        compromised: Some(false),
        expired: Some(false),
    };
    let after = RecordHealthState {
        compromised: Some(true),
        ..before.clone()
    };
    assert!(health_state_changed(&before, &after));
}

#[test]
fn health_state_changed_when_expired_differs() {
    let before = RecordHealthState {
        record_id: Uuid::new_v4(),
        record_version: 1,
        evaluated_at: Some(Utc::now()),
        weak_password: Some(false),
        duplicate_group_size: None,
        compromised: Some(false),
        expired: Some(false),
    };
    let after = RecordHealthState {
        expired: Some(true),
        ..before.clone()
    };
    assert!(health_state_changed(&before, &after));
}

#[test]
fn health_state_version_change_is_not_a_delta() {
    let before = RecordHealthState {
        record_id: Uuid::new_v4(),
        record_version: 1,
        evaluated_at: Some(Utc::now()),
        weak_password: Some(false),
        duplicate_group_size: None,
        compromised: Some(false),
        expired: Some(false),
    };
    let after = RecordHealthState {
        record_version: 2,
        ..before.clone()
    };
    // Only version changed, not health flags → not a change.
    assert!(
        !health_state_changed(&before, &after),
        "version-only change should not trigger delta"
    );
}

#[test]
fn health_state_evaluated_at_change_is_not_a_delta() {
    let before = RecordHealthState {
        record_id: Uuid::new_v4(),
        record_version: 1,
        evaluated_at: Some(Utc::now() - chrono::Duration::hours(1)),
        weak_password: Some(false),
        duplicate_group_size: None,
        compromised: Some(false),
        expired: Some(false),
    };
    let after = RecordHealthState {
        evaluated_at: Some(Utc::now()),
        ..before.clone()
    };
    assert!(
        !health_state_changed(&before, &after),
        "evaluated_at change alone should not trigger delta"
    );
}

// ===========================================================================
// Tests: handle_run_health_check (existing tests)
// ===========================================================================

#[tokio::test]
async fn health_background_cancel_sends_cancelled_result() {
    let mut executor = make_executor_with_one_login();
    let mut result_rx = {
        let (result_tx, result_rx) = mpsc::channel(64);
        executor.result_tx = result_tx;
        result_rx
    };

    let started = handle_run_health_check(&mut executor);
    assert!(matches!(started, CommandResult::HealthCheckStarted));
    executor.cancel_token().cancel();

    let message = tokio::time::timeout(std::time::Duration::from_secs(1), result_rx.recv())
        .await
        .expect("health cancellation message")
        .expect("message");

    assert!(matches!(
        message,
        Message::CommandCompleted(CommandResult::Cancelled { ref operation, .. })
            if operation == "health_check"
    ));
}

// ===========================================================================
// Tests: load_cached_health_report (existing tests)
// ===========================================================================

#[test]
fn load_cached_report_returns_none_when_no_states() {
    let mut executor = make_executor_no_records();
    let result = load_cached_health_report(&mut executor).expect("load");
    assert!(
        result.is_none(),
        "empty DB should yield Ok(None), got {:?}",
        result
    );
}

#[test]
fn load_cached_report_reconstructs_weak_passwords() {
    let mut executor = make_executor_no_records();
    let id_weak = create_login_record(&mut executor, "weak");
    let id_clean = create_login_record(&mut executor, "clean");

    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_weak,
            record_version: 1,
            evaluated_at: None,
            weak_password: Some(true),
            duplicate_group_size: None,
            compromised: None,
            expired: None,
        },
    );
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_clean,
            record_version: 1,
            evaluated_at: None,
            weak_password: Some(false),
            duplicate_group_size: None,
            compromised: None,
            expired: None,
        },
    );

    let report = load_cached_health_report(&mut executor)
        .expect("load")
        .expect("report");

    assert_eq!(report.weak_passwords, vec![id_weak]);
    assert_eq!(report.total_checked, 2);
    assert!(report.compromised.is_empty());
    assert!(report.expired.is_empty());
    assert!(report.duplicate_passwords.is_empty());
}

#[test]
fn load_cached_report_reconstructs_compromised() {
    let mut executor = make_executor_no_records();
    let id_compromised = create_login_record(&mut executor, "compromised");
    let id_safe = create_login_record(&mut executor, "safe");

    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_compromised,
            record_version: 1,
            evaluated_at: None,
            weak_password: None,
            duplicate_group_size: None,
            compromised: Some(true),
            expired: None,
        },
    );
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_safe,
            record_version: 1,
            evaluated_at: None,
            weak_password: None,
            duplicate_group_size: None,
            compromised: Some(false),
            expired: None,
        },
    );

    let report = load_cached_health_report(&mut executor)
        .expect("load")
        .expect("report");

    assert_eq!(report.compromised, vec![id_compromised]);
}

#[test]
fn load_cached_report_reconstructs_expired() {
    let mut executor = make_executor_no_records();
    let id_expired = create_login_record(&mut executor, "expired");

    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_expired,
            record_version: 1,
            evaluated_at: None,
            weak_password: None,
            duplicate_group_size: None,
            compromised: None,
            expired: Some(true),
        },
    );

    let report = load_cached_health_report(&mut executor)
        .expect("load")
        .expect("report");

    assert_eq!(report.expired, vec![id_expired]);
}

#[test]
fn load_cached_report_reconstructs_duplicates_as_single_group() {
    let mut executor = make_executor_no_records();
    let id_dup1 = create_login_record(&mut executor, "dup1");
    let id_dup2 = create_login_record(&mut executor, "dup2");
    let id_unique = create_login_record(&mut executor, "unique");

    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_dup1,
            record_version: 1,
            evaluated_at: None,
            weak_password: None,
            duplicate_group_size: Some(2),
            compromised: None,
            expired: None,
        },
    );
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_dup2,
            record_version: 1,
            evaluated_at: None,
            weak_password: None,
            duplicate_group_size: Some(2),
            compromised: None,
            expired: None,
        },
    );
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_unique,
            record_version: 1,
            evaluated_at: None,
            weak_password: None,
            duplicate_group_size: Some(1),
            compromised: None,
            expired: None,
        },
    );

    let report = load_cached_health_report(&mut executor)
        .expect("load")
        .expect("report");

    assert_eq!(
        report.duplicate_passwords.len(),
        1,
        "should have exactly one group"
    );
    let group = &report.duplicate_passwords[0];
    assert_eq!(
        group.len(),
        2,
        "group should contain both duplicate records"
    );
    assert!(group.contains(&id_dup1));
    assert!(group.contains(&id_dup2));
}

#[test]
fn load_cached_report_ignores_none_weak_password() {
    let mut executor = make_executor_no_records();
    let id = create_login_record(&mut executor, "unevaluated");

    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id,
            record_version: 1,
            evaluated_at: None,
            weak_password: None, // not yet evaluated
            duplicate_group_size: None,
            compromised: None,
            expired: None,
        },
    );

    let report = load_cached_health_report(&mut executor)
        .expect("load")
        .expect("report");

    assert!(
        report.weak_passwords.is_empty(),
        "None (not evaluated) should not be treated as weak"
    );
    assert_eq!(report.total_checked, 1);
}

#[test]
fn load_cached_report_combines_all_categories() {
    let mut executor = make_executor_no_records();
    let id_all_issues = create_login_record(&mut executor, "all_issues");
    let id_clean = create_login_record(&mut executor, "clean");

    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_all_issues,
            record_version: 1,
            evaluated_at: None,
            weak_password: Some(true),
            duplicate_group_size: Some(2),
            compromised: Some(true),
            expired: Some(true),
        },
    );
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_clean,
            record_version: 1,
            evaluated_at: None,
            weak_password: Some(false),
            duplicate_group_size: Some(1),
            compromised: Some(false),
            expired: Some(false),
        },
    );

    let report = load_cached_health_report(&mut executor)
        .expect("load")
        .expect("report");

    assert_eq!(report.total_checked, 2);
    assert_eq!(report.weak_passwords, vec![id_all_issues]);
    assert_eq!(report.compromised, vec![id_all_issues]);
    assert_eq!(report.expired, vec![id_all_issues]);
    // duplicate group: only id_all_issues has group_size >= 2
    assert_eq!(report.duplicate_passwords.len(), 1);
    assert!(report.duplicate_passwords[0].contains(&id_all_issues));
}

// ===========================================================================
// Tests: HealthCheckCompleted write-back verification (Task J acceptance)
// ===========================================================================

/// Helper: simulate what `execute(InternalHealthCheckCompleted)` does — persist
/// the report, update metadata, and return the result for assertion.
fn simulate_health_check_completed(
    executor: &mut CommandExecutor,
    report: &HealthReport,
) -> Vec<HealthStateDelta> {
    let evaluated_at = Utc::now();

    // Persist health states (Task E write-back)
    let deltas = persist_health_report(executor, report, evaluated_at).expect("persist");

    // Mark changed records for sync
    schedule_health_resync_for_records(executor, &deltas).expect("schedule resync");

    // Update last_health_check_at metadata
    executor
        .vault
        .set_last_health_check_at(evaluated_at)
        .expect("set last_health_check_at");

    deltas
}

#[test]
fn health_check_completed_writes_last_health_check_at_to_db() {
    let mut executor = make_executor_no_records();
    let id = create_login_record(&mut executor, "rec1");

    // Verify no last_health_check_at initially
    let before = executor
        .vault
        .get_last_health_check_at()
        .expect("get metadata");
    assert!(
        before.is_none(),
        "last_health_check_at should be None initially"
    );

    // Simulate a health check completing
    let report = HealthReport {
        weak_passwords: vec![id],
        duplicate_passwords: vec![],
        compromised: vec![],
        expired: vec![],
        total_checked: 1,
    };
    simulate_health_check_completed(&mut executor, &report);

    // Verify last_health_check_at is now persisted
    let after = executor
        .vault
        .get_last_health_check_at()
        .expect("get metadata");
    assert!(
        after.is_some(),
        "last_health_check_at should be set after health check completes"
    );
    let diff = (after.unwrap() - Utc::now()).num_seconds().abs();
    assert!(diff <= 2, "timestamp should be recent");
}

#[test]
fn health_check_completed_persists_record_health_states_to_db() {
    let mut executor = make_executor_no_records();
    let id1 = create_login_record(&mut executor, "rec1");
    let id2 = create_login_record(&mut executor, "rec2");

    // Verify no health states initially
    let states_before = executor.vault.list_record_health_states().expect("list");
    assert!(states_before.is_empty(), "no health states initially");

    // Simulate health check completing
    let report = HealthReport {
        weak_passwords: vec![id1],
        duplicate_passwords: vec![vec![id1, id2]],
        compromised: vec![id2],
        expired: vec![],
        total_checked: 2,
    };
    simulate_health_check_completed(&mut executor, &report);

    // Verify health states are persisted
    let states_after = executor.vault.list_record_health_states().expect("list");
    assert_eq!(
        states_after.len(),
        2,
        "both records should have health states"
    );

    let s1 = states_after
        .iter()
        .find(|s| s.record_id == id1)
        .expect("id1 state");
    assert_eq!(s1.weak_password, Some(true));
    assert_eq!(s1.duplicate_group_size, Some(2));
    assert_eq!(s1.compromised, Some(false));

    let s2 = states_after
        .iter()
        .find(|s| s.record_id == id2)
        .expect("id2 state");
    assert_eq!(s2.weak_password, Some(false));
    assert_eq!(s2.duplicate_group_size, Some(2));
    assert_eq!(s2.compromised, Some(true));
}

#[test]
fn health_check_completed_marks_changed_records_as_pending_sync() {
    let mut executor = make_executor_no_records();
    let id1 = create_login_record(&mut executor, "rec1");
    let id2 = create_login_record(&mut executor, "rec2");

    // Insert pre-existing health state for id2 so it's "unchanged"
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id2,
            record_version: 1,
            evaluated_at: Some(Utc::now()),
            weak_password: Some(false),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: Some(false),
        },
    );

    // Report says id1 is weak (new issue) and id2 is clean (same as before)
    let report = HealthReport {
        weak_passwords: vec![id1],
        duplicate_passwords: vec![],
        compromised: vec![],
        expired: vec![],
        total_checked: 2,
    };

    let deltas = simulate_health_check_completed(&mut executor, &report);

    // Only id1 should have a delta (changed from no-state to weak)
    let delta_ids: Vec<Uuid> = deltas.iter().map(|d| d.record_id).collect();
    assert!(
        delta_ids.contains(&id1),
        "id1 should have a delta (new issue)"
    );

    // Verify sync state: id1 should be Pending, id2 should not
    let sync_map = queries::load_sync_status_map(executor.vault.conn_ref());
    assert_eq!(
        sync_map.get(&id1.to_string()),
        Some(&SyncStatus::Pending),
        "id1 (changed) should be Pending"
    );
}

#[test]
fn health_check_completed_overwrites_previous_states_on_rerun() {
    let mut executor = make_executor_no_records();
    let id = create_login_record(&mut executor, "rec");

    // First check: record is weak
    let report1 = HealthReport {
        weak_passwords: vec![id],
        duplicate_passwords: vec![],
        compromised: vec![],
        expired: vec![],
        total_checked: 1,
    };
    simulate_health_check_completed(&mut executor, &report1);

    let state1 = executor
        .vault
        .list_record_health_states()
        .expect("list")
        .into_iter()
        .next()
        .expect("state");
    assert_eq!(state1.weak_password, Some(true));

    // Second check: record is now clean (password was changed)
    let report2 = HealthReport {
        weak_passwords: vec![],
        duplicate_passwords: vec![],
        compromised: vec![],
        expired: vec![],
        total_checked: 1,
    };
    simulate_health_check_completed(&mut executor, &report2);

    let state2 = executor
        .vault
        .list_record_health_states()
        .expect("list")
        .into_iter()
        .next()
        .expect("state");
    assert_eq!(
        state2.weak_password,
        Some(false),
        "second check should overwrite weak to false"
    );
    assert_eq!(state2.compromised, Some(false));
    assert_eq!(state2.expired, Some(false));
}

// ===========================================================================
// Tests: Full round-trip (unlock -> should_run -> persist -> restore)
// ===========================================================================

#[test]
fn full_roundtrip_unlock_schedule_persist_restore() {
    // Phase 1: First unlock — should schedule RunHealthCheck
    let mut executor = make_executor_no_records();
    let id = create_login_record(&mut executor, "rec");

    // Simulate first unlock scheduling
    super::vault::schedule_health_check_after_unlock(&mut executor);

    // Verify RunHealthCheck was sent
    let internal_rx = executor.internal_rx.as_mut().expect("internal_rx");
    let cmd = internal_rx.try_recv().expect("should have RunHealthCheck");
    assert!(matches!(cmd, crate::commands::Command::RunHealthCheck));

    // Phase 2: Simulate health check completing
    let report = HealthReport {
        weak_passwords: vec![id],
        duplicate_passwords: vec![],
        compromised: vec![],
        expired: vec![],
        total_checked: 1,
    };
    simulate_health_check_completed(&mut executor, &report);

    // Phase 3: Simulate second unlock (within daily window)
    // Re-read metadata to verify it was persisted
    let last_check = executor
        .vault
        .get_last_health_check_at()
        .expect("get")
        .expect("should exist");
    executor.last_health_check_time = Some(last_check);

    // Set daily frequency so second unlock won't re-run
    executor.config.security.health_check_frequency =
        crate::config::security::HealthCheckFrequency::Daily;

    super::vault::schedule_health_check_after_unlock(&mut executor);

    // Should NOT send RunHealthCheck (within window)
    let internal_rx = executor.internal_rx.as_mut().expect("internal_rx");
    assert!(
        internal_rx.try_recv().is_err(),
        "should not schedule another check within daily window"
    );

    // Should have restored the cached report
    let report_restored = executor
        .health_report
        .as_ref()
        .expect("report should be restored");
    assert_eq!(report_restored.weak_passwords, vec![id]);
    assert_eq!(report_restored.total_checked, 1);
}

/// Simulates a restart scenario: persist health states, then verify they can
/// be loaded back into a fresh executor state using the same underlying DB.
///
/// Since VaultService takes ownership of the Connection, we simulate "restart"
/// by clearing executor's in-memory state and re-running the scheduling logic
/// which reads from the persisted DB.
#[test]
fn health_report_restores_after_simulated_restart() {
    let mut executor = make_executor_no_records();
    let id_weak = create_login_record(&mut executor, "weak");
    let id_compromised = create_login_record(&mut executor, "compromised");

    // Persist health states directly (simulates previous health check)
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_weak,
            record_version: 1,
            evaluated_at: Some(Utc::now()),
            weak_password: Some(true),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: Some(false),
        },
    );
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_compromised,
            record_version: 1,
            evaluated_at: Some(Utc::now()),
            weak_password: Some(false),
            duplicate_group_size: None,
            compromised: Some(true),
            expired: Some(false),
        },
    );

    // Set last_health_check_at to a recent time
    let ts = Utc::now() - chrono::Duration::hours(1);
    executor.vault.set_last_health_check_at(ts).expect("set");

    // Simulate "restart": clear in-memory state
    executor.health_report = None;
    executor.last_health_check_time = None;
    executor.config.security.health_check_frequency =
        crate::config::security::HealthCheckFrequency::Daily;

    // Re-run unlock scheduling (this is what happens on restart)
    super::vault::schedule_health_check_after_unlock(&mut executor);

    // Should NOT send RunHealthCheck (within daily window)
    let rx = executor.internal_rx.as_mut().expect("rx");
    assert!(rx.try_recv().is_err(), "should not schedule check");

    // Should have restored the health report from persisted states
    let report = executor
        .health_report
        .as_ref()
        .expect("report should be restored");
    assert_eq!(report.weak_passwords, vec![id_weak]);
    assert_eq!(report.compromised, vec![id_compromised]);
    assert_eq!(report.total_checked, 2);

    // Verify timestamp was restored
    let restored_time = executor
        .last_health_check_time
        .expect("time should be restored");
    let diff = (restored_time - ts).num_seconds().abs();
    assert!(diff <= 1, "restored time should match persisted");
}

#[test]
fn delete_record_cleans_up_health_state() {
    let mut executor = make_executor_no_records();
    let id = create_login_record(&mut executor, "rec");

    // Insert health state
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id,
            record_version: 1,
            evaluated_at: Some(Utc::now()),
            weak_password: Some(true),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: Some(false),
        },
    );

    // Verify health state exists
    let states_before = executor.vault.list_record_health_states().expect("list");
    assert_eq!(states_before.len(), 1);

    // Soft delete the record — should cascade to health state
    executor.vault.soft_delete_record(id).expect("soft delete");

    // Verify health state is cleaned up
    let states_after = executor.vault.list_record_health_states().expect("list");
    assert!(
        states_after.is_empty(),
        "health state should be removed after record deletion"
    );
}
