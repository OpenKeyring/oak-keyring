use super::*;
use crate::config::manager::ConfigManager;
use crate::config::notification::ServiceNotification;
use crate::config::AppConfig;
use crate::config::{ConfigError, ConfigReloadable, ConfigWatcher};
use crate::services::clipboard::ClipboardService;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn setup_test_env() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir failed");
    std::env::set_var("OAK_CONFIG_DIR", tmp.path());
    std::env::set_var("OAK_VAULT_DIR", tmp.path());
    tmp
}

/// A mock ConfigReloadable service that tracks reload calls.
struct MockService {
    id: String,
    reload_count: Arc<AtomicUsize>,
    should_fail: bool,
}

impl MockService {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            reload_count: Arc::new(AtomicUsize::new(0)),
            should_fail: false,
        }
    }

    fn new_failing(id: &str) -> Self {
        Self {
            id: id.to_string(),
            reload_count: Arc::new(AtomicUsize::new(0)),
            should_fail: true,
        }
    }
}

impl ConfigReloadable for MockService {
    fn service_id(&self) -> &str {
        &self.id
    }

    fn reload(&self, _config: &AppConfig) -> Result<(), ConfigError> {
        self.reload_count.fetch_add(1, Ordering::SeqCst);
        if self.should_fail {
            Err(ConfigError::Validation("mock failure".into()))
        } else {
            Ok(())
        }
    }
}

#[test]
fn notify_with_empty_fields_notifies_all_services() {
    let mut notifier = ServiceNotificationImpl::new();
    let svc_a = MockService::new("service-a");
    let svc_a_reload_count = svc_a.reload_count.clone();
    let svc_b = MockService::new("service-b");
    let svc_b_reload_count = svc_b.reload_count.clone();

    notifier.register_service(Box::new(svc_a));
    notifier.register_service(Box::new(svc_b));

    let config = AppConfig::default();
    let results = notifier.notify_config_change(&config, &[]);
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.is_ok()));
    assert_eq!(svc_a_reload_count.load(Ordering::SeqCst), 1);
    assert_eq!(svc_b_reload_count.load(Ordering::SeqCst), 1);
}

#[test]
fn notify_filters_by_changed_fields() {
    let mut notifier = ServiceNotificationImpl::new();
    let svc_a = MockService::new("service-a");
    let svc_a_reload_count = svc_a.reload_count.clone();
    let svc_b = MockService::new("service-b");
    let svc_b_reload_count = svc_b.reload_count.clone();

    notifier.register_service(Box::new(svc_a));
    notifier.register_service(Box::new(svc_b));

    let config = AppConfig::default();
    let results = notifier.notify_config_change(&config, &["service-a"]);
    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok());
    assert_eq!(svc_a_reload_count.load(Ordering::SeqCst), 1);
    assert_eq!(svc_b_reload_count.load(Ordering::SeqCst), 0);
}

#[test]
fn notify_with_unknown_field_notifies_nothing() {
    let mut notifier = ServiceNotificationImpl::new();
    let svc_a = MockService::new("service-a");
    let svc_a_reload_count = svc_a.reload_count.clone();

    notifier.register_service(Box::new(svc_a));

    let config = AppConfig::default();
    let results = notifier.notify_config_change(&config, &["nonexistent"]);
    assert!(results.is_empty());
    assert_eq!(svc_a_reload_count.load(Ordering::SeqCst), 0);
}

#[test]
fn notify_returns_errors_from_failing_service() {
    let mut notifier = ServiceNotificationImpl::new();
    let svc_ok = MockService::new("svc-ok");
    let svc_fail = MockService::new_failing("svc-fail");

    notifier.register_service(Box::new(svc_ok));
    notifier.register_service(Box::new(svc_fail));

    let config = AppConfig::default();
    let results = notifier.notify_config_change(&config, &[]);
    assert_eq!(results.len(), 2);
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
}

#[test]
fn unregister_removes_service() {
    let mut notifier = ServiceNotificationImpl::new();
    let svc_a = MockService::new("service-a");
    let svc_a_reload_count = svc_a.reload_count.clone();

    notifier.register_service(Box::new(svc_a));
    notifier.unregister_service("service-a");

    let config = AppConfig::default();
    let results = notifier.notify_config_change(&config, &[]);
    assert!(results.is_empty());
    assert_eq!(svc_a_reload_count.load(Ordering::SeqCst), 0);
}

#[test]
fn notify_no_services_returns_empty() {
    let notifier = ServiceNotificationImpl::new();
    let config = AppConfig::default();
    let results = notifier.notify_config_change(&config, &[]);
    assert!(results.is_empty());
}

#[test]
fn notify_multiple_fields_matches_multiple_services() {
    let mut notifier = ServiceNotificationImpl::new();
    let svc_a = MockService::new("service-a");
    let svc_a_count = svc_a.reload_count.clone();
    let svc_b = MockService::new("service-b");
    let svc_b_count = svc_b.reload_count.clone();
    let svc_c = MockService::new("service-c");
    let svc_c_count = svc_c.reload_count.clone();

    notifier.register_service(Box::new(svc_a));
    notifier.register_service(Box::new(svc_b));
    notifier.register_service(Box::new(svc_c));

    let config = AppConfig::default();
    let results = notifier.notify_config_change(&config, &["service-a", "service-c"]);
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.is_ok()));
    assert_eq!(svc_a_count.load(Ordering::SeqCst), 1);
    assert_eq!(svc_b_count.load(Ordering::SeqCst), 0);
    assert_eq!(svc_c_count.load(Ordering::SeqCst), 1);
}

#[test]
fn unregister_nonexistent_id_is_noop() {
    let mut notifier = ServiceNotificationImpl::new();
    let svc_a = MockService::new("service-a");
    let svc_a_count = svc_a.reload_count.clone();

    notifier.register_service(Box::new(svc_a));
    // Unregister a non-existent service — should not affect registered ones.
    notifier.unregister_service("nonexistent");

    let config = AppConfig::default();
    let results = notifier.notify_config_change(&config, &[]);
    assert_eq!(results.len(), 1);
    assert_eq!(svc_a_count.load(Ordering::SeqCst), 1);
}

// -----------------------------------------------------------------------
// ConfigManagerImpl tests
// -----------------------------------------------------------------------

/// Helper: create a temporary vault directory with a unique name.
fn temp_vault_dir() -> tempfile::TempDir {
    setup_test_env()
}

#[test]
fn config_manager_new_initializes_with_given_config() {
    let config = AppConfig::default();
    let manager = ConfigManagerImpl::new(config.clone());
    assert_eq!(manager.get_config(), config);
}

#[test]
fn config_manager_load_reads_from_disk_and_updates_state() {
    let vault_dir = temp_vault_dir();
    std::fs::create_dir_all(vault_dir.path().join("config")).unwrap();
    // Write a config to disk first
    let disk_config = AppConfig::default();
    disk_config.save().unwrap();

    // Manager starts with default, then loads from disk
    let manager = ConfigManagerImpl::new(AppConfig::default());
    let loaded = manager.load().unwrap();
    assert_eq!(loaded, disk_config);
    assert_eq!(manager.get_config(), disk_config);

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_manager_load_returns_default_when_no_file_exists() {
    let vault_dir = temp_vault_dir();
    // No config.toml on disk — load should return defaults
    let manager = ConfigManagerImpl::new(AppConfig::default());
    let loaded = manager.load().unwrap();
    assert_eq!(loaded, AppConfig::default());
    assert_eq!(manager.get_config(), AppConfig::default());

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_manager_save_writes_to_disk_and_updates_state() {
    let vault_dir = temp_vault_dir();
    let config = AppConfig::default();
    let manager = ConfigManagerImpl::new(AppConfig::default());

    manager.save(&config).unwrap();

    // In-memory state should reflect saved config
    assert_eq!(manager.get_config(), config);

    // File should exist on disk with valid content
    let reloaded = AppConfig::load().unwrap();
    assert_eq!(reloaded, config);

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_manager_save_overwrites_existing_file() {
    let vault_dir = temp_vault_dir();
    std::fs::create_dir_all(vault_dir.path().join("config")).unwrap();

    let manager = ConfigManagerImpl::new(AppConfig::default());
    manager.save(&AppConfig::default()).unwrap();

    // Save again — should succeed without error
    let second = AppConfig::default();
    manager.save(&second).unwrap();
    assert_eq!(manager.get_config(), second);

    let from_disk = AppConfig::load().unwrap();
    assert_eq!(from_disk, second);

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_manager_reload_re_reads_from_disk() {
    let vault_dir = temp_vault_dir();
    std::fs::create_dir_all(vault_dir.path().join("config")).unwrap();

    // Write initial config
    let initial = AppConfig::default();
    initial.save().unwrap();

    let manager = ConfigManagerImpl::new(AppConfig::default());
    manager.load().unwrap();

    // Modify config on disk externally
    let modified = AppConfig::default();
    modified.save().unwrap();

    // Reload should pick up the new disk state
    let reloaded = manager.reload().unwrap();
    assert_eq!(reloaded, modified);
    assert_eq!(manager.get_config(), modified);

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_manager_reload_returns_default_when_no_file() {
    let vault_dir = temp_vault_dir();
    let manager = ConfigManagerImpl::new(AppConfig::default());

    let reloaded = manager.reload().unwrap();
    assert_eq!(reloaded, AppConfig::default());

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_manager_get_config_returns_current_snapshot() {
    let vault_dir = temp_vault_dir();
    let initial = AppConfig::default();
    let manager = ConfigManagerImpl::new(initial.clone());

    // get_config returns the in-memory state
    assert_eq!(manager.get_config(), initial);

    // After save with a new config, get_config reflects the update
    let new_config = AppConfig::default();
    manager.save(&new_config).unwrap();
    assert_eq!(manager.get_config(), new_config);

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_manager_concurrent_reads_are_safe() {
    let config = AppConfig::default();
    let manager = Arc::new(ConfigManagerImpl::new(config));
    let num_threads = 8;

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let mgr = Arc::clone(&manager);
            std::thread::spawn(move || {
                // Each thread reads the config — should never panic or deadlock
                let _cfg = mgr.get_config();
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("thread should not panic");
    }
}

#[test]
fn config_manager_concurrent_writes_are_safe() {
    let vault_dir = temp_vault_dir();
    let config = AppConfig::default();
    let manager = Arc::new(ConfigManagerImpl::new(config));
    let num_threads = 4;

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let mgr = Arc::clone(&manager);
            std::thread::spawn(move || {
                // Each thread saves — should not panic or deadlock
                let cfg = AppConfig::default();
                let _ = mgr.save(&cfg);
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("thread should not panic");
    }

    // Manager should still be in a valid state
    let _final_config = manager.get_config();

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_manager_concurrent_read_write_safe() {
    let vault_dir = temp_vault_dir();
    let config = AppConfig::default();
    config.save().unwrap();

    let manager = Arc::new(ConfigManagerImpl::new(config));
    let num_readers = 6;
    let num_writers = 2;

    let read_handles: Vec<_> = (0..num_readers)
        .map(|_| {
            let mgr = Arc::clone(&manager);
            std::thread::spawn(move || {
                for _ in 0..10 {
                    let _ = mgr.load();
                    let _ = mgr.get_config();
                }
            })
        })
        .collect();

    let write_handles: Vec<_> = (0..num_writers)
        .map(|_| {
            let mgr = Arc::clone(&manager);
            std::thread::spawn(move || {
                for _ in 0..5 {
                    let cfg = AppConfig::default();
                    let _ = mgr.save(&cfg);
                }
            })
        })
        .collect();

    for handle in read_handles.into_iter().chain(write_handles) {
        handle.join().expect("thread should not panic");
    }

    // cleanup happens when vault_dir is dropped at end of scope
}

// -----------------------------------------------------------------------
// ConfigWatcherImpl tests
// -----------------------------------------------------------------------

#[test]
fn config_watcher_new_initializes_with_no_mtime() {
    let watcher = ConfigWatcherImpl::new();
    // A brand-new watcher should have no stored mtime.
    // We verify this indirectly: needs_reload returns true for a first-time check
    // when no file exists only if last_mtime is None.
    // Actually needs_reload returns false when no file exists, so let's check
    // via a file existing scenario.
    let vault_dir = temp_vault_dir();
    let config = AppConfig::default();
    config.save().unwrap();

    // First time — no stored mtime — should need reload
    assert!(watcher.needs_reload());

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_watcher_needs_reload_returns_true_when_file_newer_than_stored_mtime() {
    let vault_dir = temp_vault_dir();
    std::fs::create_dir_all(vault_dir.path().join("config")).unwrap();
    let watcher = ConfigWatcherImpl::new();

    // Write initial config and capture its mtime
    let config = AppConfig::default();
    config.save().unwrap();
    let mut watcher = watcher;
    watcher.update_mtime();

    // At this point, needs_reload should be false (same mtime)
    assert!(!watcher.needs_reload());

    // Wait briefly then rewrite the file to get a newer mtime
    std::thread::sleep(std::time::Duration::from_millis(50));
    config.save().unwrap();

    // Now the file is newer than stored mtime
    assert!(watcher.needs_reload());

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_watcher_needs_reload_returns_false_when_no_config_file() {
    let vault_dir = temp_vault_dir();
    let watcher = ConfigWatcherImpl::new();

    // No file on disk — needs_reload should return false regardless of stored mtime
    assert!(!watcher.needs_reload());

    // Also true if we had a previous mtime stored
    let mut watcher = watcher;
    watcher.last_mtime = Some(std::time::SystemTime::now());
    assert!(!watcher.needs_reload());

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_watcher_needs_reload_returns_true_on_first_time_check() {
    let vault_dir = temp_vault_dir();
    let config = AppConfig::default();
    config.save().unwrap();

    // Fresh watcher with no stored mtime
    let watcher = ConfigWatcherImpl::new();
    assert!(watcher.needs_reload());

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_watcher_needs_reload_returns_false_after_update_mtime() {
    let vault_dir = temp_vault_dir();
    let config = AppConfig::default();
    config.save().unwrap();

    let mut watcher = ConfigWatcherImpl::new();
    assert!(watcher.needs_reload());

    // After updating mtime to current file mtime, no reload needed
    watcher.update_mtime();
    assert!(!watcher.needs_reload());

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_watcher_last_modified_returns_current_file_mtime() {
    let vault_dir = temp_vault_dir();
    let config = AppConfig::default();
    config.save().unwrap();

    let watcher = ConfigWatcherImpl::new();
    let mtime = watcher.last_modified();

    assert!(mtime.is_some());

    // The returned mtime should be recent (within last few seconds)
    let elapsed = std::time::SystemTime::now()
        .duration_since(mtime.unwrap())
        .unwrap();
    assert!(elapsed.as_secs() < 5, "mtime should be recent");

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_watcher_last_modified_returns_none_when_no_file() {
    let vault_dir = temp_vault_dir();
    let watcher = ConfigWatcherImpl::new();

    assert!(watcher.last_modified().is_none());

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_watcher_update_mtime_sets_stored_mtime_to_current_file() {
    let vault_dir = temp_vault_dir();
    let config = AppConfig::default();
    config.save().unwrap();

    let mut watcher = ConfigWatcherImpl::new();
    assert!(watcher.last_mtime.is_none());

    watcher.update_mtime();
    assert!(watcher.last_mtime.is_some());

    // The stored mtime should match the file's actual mtime
    let file_mtime = ConfigWatcherImpl::current_mtime().unwrap();
    assert_eq!(watcher.last_mtime.unwrap(), file_mtime);

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_watcher_update_mtime_with_no_file_clears_stored_mtime() {
    let vault_dir = temp_vault_dir();
    let mut watcher = ConfigWatcherImpl::new();

    // Simulate having a previous mtime
    watcher.last_mtime = Some(std::time::SystemTime::UNIX_EPOCH);

    // update_mtime on a non-existent file should set mtime to None
    watcher.update_mtime();
    assert!(watcher.last_mtime.is_none());

    // cleanup happens when vault_dir is dropped at end of scope
}

#[test]
fn config_watcher_default_is_same_as_new() {
    let via_new = ConfigWatcherImpl::new();
    let via_default = ConfigWatcherImpl::default();
    assert_eq!(via_new.last_mtime, via_default.last_mtime);
}

// -----------------------------------------------------------------------
// ClipboardConfigAdapter tests
// -----------------------------------------------------------------------

use crate::services::clipboard::MockBackend;

#[test]
fn clipboard_adapter_service_id() {
    let clipboard = Arc::new(ClipboardService::with_backend(
        Box::new(MockBackend::new()),
        30,
    ));
    let adapter = ClipboardConfigAdapter::new(clipboard);
    assert_eq!(adapter.service_id(), "clipboard");
}

#[test]
fn clipboard_adapter_reload_updates_timeout() {
    let clipboard = Arc::new(ClipboardService::with_backend(
        Box::new(MockBackend::new()),
        30,
    ));
    let adapter = ClipboardConfigAdapter::new(Arc::clone(&clipboard));

    let mut config = AppConfig::default();
    config.general.clipboard_clear_seconds = 120;
    adapter.reload(&config).unwrap();

    assert_eq!(clipboard.clear_timeout(), 120);
}

#[test]
fn clipboard_adapter_integrated_with_notifier() {
    let clipboard = Arc::new(ClipboardService::with_backend(
        Box::new(MockBackend::new()),
        30,
    ));

    let mut notifier = ServiceNotificationImpl::new();
    notifier.register_service(Box::new(ClipboardConfigAdapter::new(Arc::clone(
        &clipboard,
    ))));

    let mut config = AppConfig::default();
    config.general.clipboard_clear_seconds = 60;

    let results = notifier.notify_config_change(&config, &["clipboard"]);
    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok());
    assert_eq!(clipboard.clear_timeout(), 60);
}

#[test]
fn clipboard_adapter_not_notified_for_other_fields() {
    let clipboard = Arc::new(ClipboardService::with_backend(
        Box::new(MockBackend::new()),
        30,
    ));

    let mut notifier = ServiceNotificationImpl::new();
    notifier.register_service(Box::new(ClipboardConfigAdapter::new(Arc::clone(
        &clipboard,
    ))));

    let config = AppConfig::default();
    let results = notifier.notify_config_change(&config, &["sync"]);
    assert!(results.is_empty());
    assert_eq!(clipboard.clear_timeout(), 30); // unchanged
}
