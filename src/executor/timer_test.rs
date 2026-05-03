use crate::config::sync::{SyncMode, SyncProvider};
use crate::config::AppConfig;
use crate::executor::timer::ExecutorTimers;

fn make_config(provider: SyncProvider, mode: SyncMode, interval_secs: u64) -> AppConfig {
    let mut config = AppConfig::default();
    config.sync.provider = provider;
    config.sync.sync_mode = mode;
    config.sync.auto_interval_seconds = interval_secs;
    config.general.auto_lock_seconds = 0;
    config
}

#[test]
fn sync_interval_none_when_provider_disabled() {
    let config = make_config(SyncProvider::Disabled, SyncMode::Auto, 600);
    let timers = ExecutorTimers::new(&config, true);
    assert!(
        timers.sync_interval.is_none(),
        "sync_interval should be None when provider is Disabled"
    );
    assert!(!timers.sync_active);
}

#[test]
fn sync_interval_none_when_sync_service_unavailable() {
    let config = make_config(SyncProvider::ICloud, SyncMode::Auto, 600);
    let timers = ExecutorTimers::new(&config, false);
    assert!(
        timers.sync_interval.is_none(),
        "sync_interval should be None when sync_service_available is false"
    );
    assert!(!timers.sync_active);
}

#[test]
fn sync_interval_none_when_manual_mode() {
    let config = make_config(SyncProvider::ICloud, SyncMode::Manual, 600);
    let timers = ExecutorTimers::new(&config, true);
    assert!(
        timers.sync_interval.is_none(),
        "sync_interval should be None when sync_mode is Manual"
    );
    assert!(!timers.sync_active);
}

#[tokio::test]
async fn sync_interval_some_when_all_conditions_met() {
    let config = make_config(SyncProvider::ICloud, SyncMode::Auto, 600);
    let timers = ExecutorTimers::new(&config, true);
    assert!(
        timers.sync_interval.is_some(),
        "sync_interval should be Some when all conditions are met"
    );
    assert!(timers.sync_active);
}

#[test]
fn sync_interval_none_when_zero_interval() {
    let config = make_config(SyncProvider::ICloud, SyncMode::Auto, 0);
    let timers = ExecutorTimers::new(&config, true);
    assert!(
        timers.sync_interval.is_none(),
        "sync_interval should be None when auto_interval_seconds is 0"
    );
    assert!(!timers.sync_active);
}
