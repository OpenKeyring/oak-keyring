use crate::config::sync::{SyncMode, SyncProvider};
use crate::config::AppConfig;
use crate::executor::runtime::VaultRuntime;
use crate::executor::timer::ExecutorTimers;
use crate::executor::CommandExecutor;
use crate::services::vault::MockVault;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

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

fn executor_with_vault(unlocked: bool) -> CommandExecutor {
    let (result_tx, _) = mpsc::channel(8);
    let mut vault = MockVault::new();
    vault.expect_is_unlocked().returning(move || unlocked);

    CommandExecutor::builder(":memory:".into(), ":memory:".into())
        .vault(Box::new(vault))
        .vault_db_file_backed(true)
        .config(AppConfig::default())
        .result_tx(result_tx)
        .shutdown_token(CancellationToken::new())
        .build()
        .expect("executor should build")
}

#[test]
fn auto_timers_do_not_emit_commands_when_runtime_is_locked() {
    let (result_tx, _) = mpsc::channel(8);
    let executor = CommandExecutor::builder(":memory:".into(), ":memory:".into())
        .vault_runtime(VaultRuntime::locked())
        .vault_db_file_backed(true)
        .config(AppConfig::default())
        .result_tx(result_tx)
        .shutdown_token(CancellationToken::new())
        .build()
        .expect("executor should build");

    assert!(!executor.should_run_auto_sync_timer());
    assert!(!executor.should_run_auto_lock_timer());
}

#[test]
fn auto_timers_do_not_emit_commands_when_file_backed_vault_is_locked() {
    let executor = executor_with_vault(false);

    assert!(!executor.should_run_auto_sync_timer());
    assert!(!executor.should_run_auto_lock_timer());
}

#[test]
fn auto_timers_emit_commands_when_vault_is_unlocked() {
    let executor = executor_with_vault(true);

    assert!(executor.should_run_auto_sync_timer());
    assert!(executor.should_run_auto_lock_timer());
}
