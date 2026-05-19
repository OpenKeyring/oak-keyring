//! Integration tests for DEK rotation acceptance criteria.
//!
//! | AC  | Rule                                            | Test                                  |
//! |-----|-------------------------------------------------|---------------------------------------|
//! | AC1 | auto time trigger after 90 days                | acceptance_auto_time_trigger_after_90_days
//! | AC2 | auto count trigger at 1000 records             | acceptance_auto_count_trigger_at_1000_records
//! | AC3 | offline rotation skipped                       | acceptance_offline_rotation_skipped
//! | AC4 | auto_rotate disabled skips auto triggers       | acceptance_auto_rotate_disabled_skips_auto_triggers
//! | AC5 | MAX_DEK_VERSION constant is 10000              | acceptance_max_dek_version_constant_is_10000
//! | AC6 | GRACE_PERIOD constant is 24 hours              | acceptance_grace_period_constant_is_24_hours
//! | AC7 | SYNC_PAUSE_TIMEOUT constant is 30 seconds      | acceptance_sync_pause_timeout_constant_is_30_seconds
//! | AC8 | cloud version newer -> skip rotation           | acceptance_should_skip_when_cloud_is_newer
//! | AC9 | grace period boundary at 24h                   | acceptance_grace_period_boundary
//! | AC10| RotationService starts Idle                    | acceptance_rotation_service_starts_idle
//! | AC11| rotation config defaults (auto_rotate=true, 90d, 1000r) | acceptance_rotation_config_defaults

use oak_keyring::db::schema::init_db_in_memory;
use oak_keyring::services::rotation::{
    check_trigger, is_past_grace_period, should_skip_rotation_due_to_cloud_version, RotationService,
};
use oak_keyring::services::vault::VaultService;
use oak_keyring::types::rotation::{
    RotationConfig, RotationConstants, RotationState, RotationTrigger,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create an in-memory VaultService with schema initialized.
fn setup_vault() -> VaultService {
    let conn = init_db_in_memory().unwrap();
    VaultService::new(conn)
}

// ---------------------------------------------------------------------------
// AC1: Auto time trigger after 90 days
// ---------------------------------------------------------------------------

#[test]
fn acceptance_auto_time_trigger_after_90_days() {
    let config = RotationConfig {
        auto_rotate: true,
        rotate_after_days: Some(90),
        rotate_after_records: None,
        last_rotation_at: None,
        current_dek_record_count: 0,
    };
    let result = check_trigger(&config, true, Some(90), 0);
    assert!(
        matches!(
            result,
            Some(RotationTrigger::AutoTime {
                days_since_last: 90
            })
        ),
        "expected AutoTime trigger at 90 days"
    );
}

// ---------------------------------------------------------------------------
// AC2: Auto count trigger at 1000 records
// ---------------------------------------------------------------------------

#[test]
fn acceptance_auto_count_trigger_at_1000_records() {
    let config = RotationConfig {
        auto_rotate: true,
        rotate_after_days: None,
        rotate_after_records: Some(1000),
        last_rotation_at: None,
        current_dek_record_count: 1000,
    };
    let result = check_trigger(&config, true, None, 1000);
    assert!(
        matches!(
            result,
            Some(RotationTrigger::AutoCount { record_count: 1000 })
        ),
        "expected AutoCount trigger at 1000 records"
    );
}

// ---------------------------------------------------------------------------
// AC3: Offline rotation skipped
// ---------------------------------------------------------------------------

#[test]
fn acceptance_offline_rotation_skipped() {
    let config = RotationConfig {
        auto_rotate: true,
        rotate_after_days: Some(90),
        rotate_after_records: None,
        last_rotation_at: None,
        current_dek_record_count: 0,
    };
    let result = check_trigger(&config, false, Some(90), 0);
    assert!(result.is_none(), "offline rotation should be skipped");
}

// ---------------------------------------------------------------------------
// AC4: auto_rotate disabled skips auto triggers
// ---------------------------------------------------------------------------

#[test]
fn acceptance_auto_rotate_disabled_skips_auto_triggers() {
    let config = RotationConfig {
        auto_rotate: false,
        rotate_after_days: Some(90),
        rotate_after_records: Some(1000),
        last_rotation_at: None,
        current_dek_record_count: 1000,
    };
    let result = check_trigger(&config, true, Some(90), 1000);
    assert!(
        result.is_none(),
        "auto triggers should be skipped when disabled"
    );
}

// ---------------------------------------------------------------------------
// AC5: MAX_DEK_VERSION constant is 10000
// ---------------------------------------------------------------------------

#[test]
fn acceptance_max_dek_version_constant_is_10000() {
    assert_eq!(RotationConstants::MAX_DEK_VERSION, 10_000);
}

// ---------------------------------------------------------------------------
// AC6: GRACE_PERIOD constant is 24 hours
// ---------------------------------------------------------------------------

#[test]
fn acceptance_grace_period_constant_is_24_hours() {
    assert_eq!(RotationConstants::GRACE_PERIOD_HOURS, 24);
}

// ---------------------------------------------------------------------------
// AC7: SYNC_PAUSE_TIMEOUT constant is 30 seconds
// ---------------------------------------------------------------------------

#[test]
fn acceptance_sync_pause_timeout_constant_is_30_seconds() {
    assert_eq!(RotationConstants::SYNC_PAUSE_TIMEOUT_SECS, 30);
}

// ---------------------------------------------------------------------------
// AC8: cloud version newer -> skip rotation
// ---------------------------------------------------------------------------

#[test]
fn acceptance_should_skip_when_cloud_is_newer() {
    assert!(
        should_skip_rotation_due_to_cloud_version(1, 2),
        "should skip when cloud version is newer"
    );
    assert!(
        !should_skip_rotation_due_to_cloud_version(2, 1),
        "should not skip when local version is newer"
    );
    assert!(
        !should_skip_rotation_due_to_cloud_version(1, 1),
        "should not skip when versions are equal"
    );
}

// ---------------------------------------------------------------------------
// AC9: grace period boundary at 24h
// ---------------------------------------------------------------------------

#[test]
fn acceptance_grace_period_boundary() {
    // Past 24 hours -> should be past grace period
    let triggered_25h_ago = chrono::Utc::now() - chrono::Duration::hours(25);
    assert!(
        is_past_grace_period(triggered_25h_ago),
        "25 hours ago should be past grace period"
    );

    // Within 24 hours -> should not be past grace period
    let triggered_23h_ago = chrono::Utc::now() - chrono::Duration::hours(23);
    assert!(
        !is_past_grace_period(triggered_23h_ago),
        "23 hours ago should not be past grace period"
    );
}

// ---------------------------------------------------------------------------
// AC10: RotationService starts Idle
// ---------------------------------------------------------------------------

#[test]
fn acceptance_rotation_service_starts_idle() {
    let mut vault = setup_vault();
    let service = RotationService::new(&mut vault);
    assert!(
        matches!(service.state(), RotationState::Idle),
        "new RotationService should start in Idle state"
    );
}

// ---------------------------------------------------------------------------
// AC11: rotation config defaults (auto_rotate=true, 90d, 1000r)
// ---------------------------------------------------------------------------

#[test]
fn acceptance_rotation_config_defaults() {
    let mut vault = setup_vault();
    let service = RotationService::new(&mut vault);
    let config = service.get_config().unwrap();
    assert!(config.auto_rotate, "auto_rotate should default to true");
    assert_eq!(
        config.rotate_after_days,
        Some(90),
        "rotate_after_days should default to 90"
    );
    assert_eq!(
        config.rotate_after_records,
        Some(1000),
        "rotate_after_records should default to 1000"
    );
}
