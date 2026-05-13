use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::config::security::HealthCheckFrequency;
use crate::config::AppConfig;
use crate::crypto::bip39::{MnemonicLanguage, Passkey};
use crate::executor::config_impl::ServiceNotificationImpl;
use crate::executor::vault::schedule_health_check_after_unlock;
use crate::executor::CommandExecutor;
use crate::services::clipboard::{ClipboardService, MockBackend};
use crate::services::health::HealthService;
use crate::services::import_export::ImportExportService;
use crate::services::vault::VaultService;
use crate::types::health::RecordHealthState;
use crate::types::{CredentialType, EncryptedPayload, SecureStr};

/// Create a basic unlocked executor with no records.
fn make_unlocked_executor() -> CommandExecutor {
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
        config: crate::executor::config_impl::ConfigManagerImpl::new(
            AppConfig::default(),
            std::path::PathBuf::from(":memory:"),
        ),
        config_notifier: ServiceNotificationImpl::new(),
        vault_dir: std::path::PathBuf::from(":memory:"),
        config_dir: std::path::PathBuf::from(":memory:"),
        health_report: None,
        last_health_check_time: None,
        result_tx,
        internal_tx,
        internal_rx: Some(internal_rx),
        shutdown_token: CancellationToken::new(),
        operation_cancel_token: CancellationToken::new(),
        timer_rebuild_pending: false,
        oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
        verified_master_password: None,
    }
}

/// Helper: create a Login record and return its UUID.
fn create_login_record(executor: &mut CommandExecutor, name: &str) -> uuid::Uuid {
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

// =========================================================================
// schedule_health_check_after_unlock tests
// =========================================================================

// --- schedules RunHealthCheck when should_run is true (OnStartup, no last check) ---

#[test]
fn schedules_run_health_check_when_no_previous_check() {
    let mut executor = make_unlocked_executor();
    // Default config: health_check_enabled=true, frequency=OnStartup
    // No last_health_check_at set → should_run returns true

    schedule_health_check_after_unlock(&mut executor);

    // Verify ScheduleHealthCheck was sent to the internal channel
    let internal_rx = executor.internal_rx.as_mut().expect("internal_rx");
    let cmd = internal_rx
        .try_recv()
        .expect("should have a command in internal channel");
    assert!(
        matches!(
            cmd,
            crate::commands::InternalCommand::ScheduleHealthCheck { .. }
        ),
        "expected ScheduleHealthCheck command, got {:?}",
        cmd
    );

    // health_report should remain None (not loaded from cache)
    assert!(executor.health_report.is_none());
}

// --- loads cached report when health check is not due ---

#[test]
fn loads_cached_report_when_check_not_due() {
    let mut executor = make_unlocked_executor();

    // Set frequency to Weekly and set last check to recent (within window)
    executor.config.update_config_for_test(|c| {
        c.security.health_check_frequency = HealthCheckFrequency::Weekly
    });
    let recent = chrono::Utc::now() - chrono::Duration::hours(1);
    executor
        .vault
        .set_last_health_check_at(recent)
        .expect("set last health check");

    // Insert some health state data
    let id = create_login_record(&mut executor, "test");
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id,
            record_version: 1,
            evaluated_at: None,
            weak_password: Some(true),
            duplicate_group_size: None,
            compromised: None,
            expired: None,
        },
    );

    schedule_health_check_after_unlock(&mut executor);

    // Should NOT have sent RunHealthCheck
    let internal_rx = executor.internal_rx.as_mut().expect("internal_rx");
    assert!(
        internal_rx.try_recv().is_err(),
        "internal channel should be empty — no ScheduleHealthCheck sent"
    );

    // Should have loaded cached report
    let report = executor
        .health_report
        .as_ref()
        .expect("health_report should be set");
    assert_eq!(report.weak_passwords, vec![id]);
    assert_eq!(report.total_checked, 1);

    // last_health_check_time should be restored
    let restored_time = executor
        .last_health_check_time
        .expect("time should be restored");
    let diff = (restored_time - recent).num_seconds().abs();
    assert!(diff <= 1, "restored time should match persisted value");
}

// --- does nothing when health check is disabled ---

#[test]
fn does_nothing_when_health_check_disabled() {
    let mut executor = make_unlocked_executor();
    executor
        .config
        .update_config_for_test(|c| c.security.health_check_enabled = false);

    schedule_health_check_after_unlock(&mut executor);

    // Should NOT have sent RunHealthCheck
    let internal_rx = executor.internal_rx.as_mut().expect("internal_rx");
    assert!(
        internal_rx.try_recv().is_err(),
        "internal channel should be empty when health check disabled"
    );

    // Should NOT have loaded any report
    assert!(executor.health_report.is_none());
}

// --- handles missing metadata gracefully (no last_health_check_at) ---

#[test]
fn handles_missing_metadata_gracefully() {
    let mut executor = make_unlocked_executor();
    // Default executor has no last_health_check_at metadata
    // With OnStartup frequency, should_run returns true → schedules check

    schedule_health_check_after_unlock(&mut executor);

    // ScheduleHealthCheck should have been sent
    let internal_rx = executor.internal_rx.as_mut().expect("internal_rx");
    let cmd = internal_rx
        .try_recv()
        .expect("should have ScheduleHealthCheck");
    assert!(matches!(
        cmd,
        crate::commands::InternalCommand::ScheduleHealthCheck { .. }
    ));
}

// --- loads empty report gracefully when no health states exist ---

#[test]
fn loads_empty_cache_when_no_health_states() {
    let mut executor = make_unlocked_executor();

    // Set frequency to Daily and set last check to recent
    executor.config.update_config_for_test(|c| {
        c.security.health_check_frequency = HealthCheckFrequency::Daily
    });
    let recent = chrono::Utc::now() - chrono::Duration::minutes(30);
    executor
        .vault
        .set_last_health_check_at(recent)
        .expect("set last health check");

    // No health states inserted → load_cached_health_report returns Ok(None)

    schedule_health_check_after_unlock(&mut executor);

    // Should NOT have sent RunHealthCheck
    let internal_rx = executor.internal_rx.as_mut().expect("internal_rx");
    assert!(
        internal_rx.try_recv().is_err(),
        "internal channel should be empty"
    );

    // health_report should remain None (no cached data)
    assert!(executor.health_report.is_none());
}

// --- schedules check when Daily frequency and last check was 2 days ago ---

#[test]
fn schedules_check_when_daily_frequency_expired() {
    let mut executor = make_unlocked_executor();
    executor.config.update_config_for_test(|c| {
        c.security.health_check_frequency = HealthCheckFrequency::Daily
    });

    // Set last check to 2 days ago (> 24h)
    let two_days_ago = chrono::Utc::now() - chrono::Duration::days(2);
    executor
        .vault
        .set_last_health_check_at(two_days_ago)
        .expect("set last health check");

    schedule_health_check_after_unlock(&mut executor);

    let internal_rx = executor.internal_rx.as_mut().expect("internal_rx");
    let cmd = internal_rx
        .try_recv()
        .expect("should have ScheduleHealthCheck");
    assert!(matches!(
        cmd,
        crate::commands::InternalCommand::ScheduleHealthCheck { .. }
    ));
}

// --- loads cached report with multiple categories ---

#[test]
fn loads_cached_report_with_multiple_categories() {
    let mut executor = make_unlocked_executor();

    executor.config.update_config_for_test(|c| {
        c.security.health_check_frequency = HealthCheckFrequency::Weekly
    });
    let recent = chrono::Utc::now() - chrono::Duration::hours(6);
    executor
        .vault
        .set_last_health_check_at(recent)
        .expect("set last health check");

    let id_weak = create_login_record(&mut executor, "weak");
    let id_compromised = create_login_record(&mut executor, "compromised");
    let id_clean = create_login_record(&mut executor, "clean");

    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_weak,
            record_version: 1,
            evaluated_at: None,
            weak_password: Some(true),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: None,
        },
    );
    insert_health_state(
        &mut executor,
        RecordHealthState {
            record_id: id_compromised,
            record_version: 1,
            evaluated_at: None,
            weak_password: Some(false),
            duplicate_group_size: None,
            compromised: Some(true),
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
            compromised: Some(false),
            expired: None,
        },
    );

    schedule_health_check_after_unlock(&mut executor);

    // No RunHealthCheck sent
    let internal_rx = executor.internal_rx.as_mut().expect("internal_rx");
    assert!(internal_rx.try_recv().is_err());

    let report = executor.health_report.as_ref().expect("report");
    assert_eq!(report.total_checked, 3);
    assert_eq!(report.weak_passwords, vec![id_weak]);
    assert_eq!(report.compromised, vec![id_compromised]);
}
