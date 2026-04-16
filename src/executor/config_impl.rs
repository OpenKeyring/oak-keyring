//! Concrete implementations of ConfigManager, ConfigWatcher, and ServiceNotification.
//!
//! These structs implement the D3 config traits for use by the S5 executor layer.

use std::path::Path;
use std::sync::RwLock;
use std::time::SystemTime;

use crate::config::{
    AppConfig, ConfigError, ConfigManager, ConfigReloadable, ConfigWatcher, ServiceNotification,
};

// ---------------------------------------------------------------------------
// ConfigManagerImpl
// ---------------------------------------------------------------------------

/// Thread-safe in-memory config holder that persists changes to disk.
pub struct ConfigManagerImpl {
    config: RwLock<AppConfig>,
}

impl ConfigManagerImpl {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: RwLock::new(config),
        }
    }
}

impl ConfigManager for ConfigManagerImpl {
    fn load(&self, vault_dir: &Path) -> Result<AppConfig, ConfigError> {
        let config = AppConfig::load(vault_dir)?;
        let mut current = self.config.write().unwrap();
        *current = config.clone();
        Ok(config)
    }

    fn save(&self, config: &AppConfig, vault_dir: &Path) -> Result<(), ConfigError> {
        config.save(vault_dir)?;
        let mut current = self.config.write().unwrap();
        *current = config.clone();
        Ok(())
    }

    fn reload(&self, vault_dir: &Path) -> Result<AppConfig, ConfigError> {
        let config = AppConfig::load(vault_dir)?;
        let mut current = self.config.write().unwrap();
        *current = config.clone();
        Ok(config)
    }

    fn get_config(&self) -> AppConfig {
        self.config.read().unwrap().clone()
    }
}

// ---------------------------------------------------------------------------
// ConfigWatcherImpl
// ---------------------------------------------------------------------------

/// Polling-based config file change detector using mtime comparison.
pub struct ConfigWatcherImpl {
    last_mtime: Option<SystemTime>,
}

impl ConfigWatcherImpl {
    pub fn new() -> Self {
        Self { last_mtime: None }
    }

    fn current_mtime(vault_dir: &Path) -> Option<SystemTime> {
        let config_path = vault_dir.join("config.toml");
        std::fs::metadata(&config_path).ok()?.modified().ok()
    }
}

impl Default for ConfigWatcherImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigWatcher for ConfigWatcherImpl {
    fn needs_reload(&self, vault_dir: &Path) -> bool {
        let current = Self::current_mtime(vault_dir);
        match (current, self.last_mtime) {
            (Some(now), Some(last)) => now > last,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    fn last_modified(&self, vault_dir: &Path) -> Option<SystemTime> {
        Self::current_mtime(vault_dir)
    }
}

// Note: ConfigWatcherImpl::needs_reload is read-only (&self) per the trait.
// To update the stored mtime after a reload, callers should drop and recreate
// the watcher, or we provide a separate update method.

impl ConfigWatcherImpl {
    /// Update the stored mtime snapshot to the current file mtime.
    /// Call this after a successful reload to avoid repeated reload triggers.
    pub fn update_mtime(&mut self, vault_dir: &Path) {
        self.last_mtime = Self::current_mtime(vault_dir);
    }
}

// ---------------------------------------------------------------------------
// ServiceNotificationImpl
// ---------------------------------------------------------------------------

/// Registry of services that need to be notified on config changes.
pub struct ServiceNotificationImpl {
    services: Vec<Box<dyn ConfigReloadable>>,
}

impl ServiceNotificationImpl {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
        }
    }
}

impl Default for ServiceNotificationImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceNotification for ServiceNotificationImpl {
    fn notify_config_change(
        &self,
        config: &AppConfig,
        changed_fields: &[&str],
    ) -> Vec<Result<(), ConfigError>> {
        self.services
            .iter()
            .filter(|service| {
                // Empty changed_fields means full reload — notify every service.
                if changed_fields.is_empty() {
                    return true;
                }
                // Only notify services whose ID appears in the changed_fields list.
                changed_fields
                    .iter()
                    .any(|field| *field == service.service_id())
            })
            .map(|service| {
                tracing::debug!(
                    service_id = service.service_id(),
                    changed_fields = ?changed_fields,
                    "Notifying service of config change"
                );
                service.reload(config)
            })
            .collect()
    }

    fn register_service(&mut self, service: Box<dyn ConfigReloadable>) {
        tracing::info!(
            service_id = service.service_id(),
            "Registering service for config notifications"
        );
        self.services.push(service);
    }

    fn unregister_service(&mut self, service_id: &str) {
        tracing::info!(
            service_id = service_id,
            "Unregistering service from config notifications"
        );
        self.services.retain(|s| s.service_id() != service_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

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
    fn temp_vault_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ok_config_impl_test_{}", uuid::Uuid::new_v4()))
    }

    /// Helper: clean up temp dir if it exists.
    fn cleanup_dir(dir: &std::path::Path) {
        let _ = std::fs::remove_dir_all(dir);
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
        // Write a config to disk first
        let disk_config = AppConfig::default();
        disk_config.save(&vault_dir).unwrap();

        // Manager starts with default, then loads from disk
        let manager = ConfigManagerImpl::new(AppConfig::default());
        let loaded = manager.load(&vault_dir).unwrap();
        assert_eq!(loaded, disk_config);
        assert_eq!(manager.get_config(), disk_config);

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_manager_load_returns_default_when_no_file_exists() {
        let vault_dir = temp_vault_dir();
        // No config.toml on disk — load should return defaults
        let manager = ConfigManagerImpl::new(AppConfig::default());
        let loaded = manager.load(&vault_dir).unwrap();
        assert_eq!(loaded, AppConfig::default());
        assert_eq!(manager.get_config(), AppConfig::default());

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_manager_save_writes_to_disk_and_updates_state() {
        let vault_dir = temp_vault_dir();
        let config = AppConfig::default();
        let manager = ConfigManagerImpl::new(AppConfig::default());

        manager.save(&config, &vault_dir).unwrap();

        // In-memory state should reflect saved config
        assert_eq!(manager.get_config(), config);

        // File should exist on disk with valid content
        let reloaded = AppConfig::load(&vault_dir).unwrap();
        assert_eq!(reloaded, config);

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_manager_save_overwrites_existing_file() {
        let vault_dir = temp_vault_dir();

        let manager = ConfigManagerImpl::new(AppConfig::default());
        manager.save(&AppConfig::default(), &vault_dir).unwrap();

        // Save again — should succeed without error
        let second = AppConfig::default();
        manager.save(&second, &vault_dir).unwrap();
        assert_eq!(manager.get_config(), second);

        let from_disk = AppConfig::load(&vault_dir).unwrap();
        assert_eq!(from_disk, second);

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_manager_reload_re_reads_from_disk() {
        let vault_dir = temp_vault_dir();

        // Write initial config
        let initial = AppConfig::default();
        initial.save(&vault_dir).unwrap();

        let manager = ConfigManagerImpl::new(AppConfig::default());
        manager.load(&vault_dir).unwrap();

        // Modify config on disk externally
        let modified = AppConfig::default();
        modified.save(&vault_dir).unwrap();

        // Reload should pick up the new disk state
        let reloaded = manager.reload(&vault_dir).unwrap();
        assert_eq!(reloaded, modified);
        assert_eq!(manager.get_config(), modified);

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_manager_reload_returns_default_when_no_file() {
        let vault_dir = temp_vault_dir();
        let manager = ConfigManagerImpl::new(AppConfig::default());

        let reloaded = manager.reload(&vault_dir).unwrap();
        assert_eq!(reloaded, AppConfig::default());

        cleanup_dir(&vault_dir);
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
        manager.save(&new_config, &vault_dir).unwrap();
        assert_eq!(manager.get_config(), new_config);

        cleanup_dir(&vault_dir);
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
                let dir = vault_dir.clone();
                std::thread::spawn(move || {
                    // Each thread saves — should not panic or deadlock
                    let cfg = AppConfig::default();
                    let _ = mgr.save(&cfg, &dir);
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread should not panic");
        }

        // Manager should still be in a valid state
        let _final_config = manager.get_config();

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_manager_concurrent_read_write_safe() {
        let vault_dir = temp_vault_dir();
        let config = AppConfig::default();
        config.save(&vault_dir).unwrap();

        let manager = Arc::new(ConfigManagerImpl::new(config));
        let num_readers = 6;
        let num_writers = 2;

        let read_handles: Vec<_> = (0..num_readers)
            .map(|_| {
                let mgr = Arc::clone(&manager);
                let dir = vault_dir.clone();
                std::thread::spawn(move || {
                    for _ in 0..10 {
                        let _ = mgr.load(&dir);
                        let _ = mgr.get_config();
                    }
                })
            })
            .collect();

        let write_handles: Vec<_> = (0..num_writers)
            .map(|_| {
                let mgr = Arc::clone(&manager);
                let dir = vault_dir.clone();
                std::thread::spawn(move || {
                    for _ in 0..5 {
                        let cfg = AppConfig::default();
                        let _ = mgr.save(&cfg, &dir);
                    }
                })
            })
            .collect();

        for handle in read_handles.into_iter().chain(write_handles) {
            handle.join().expect("thread should not panic");
        }

        cleanup_dir(&vault_dir);
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
        config.save(&vault_dir).unwrap();

        // First time — no stored mtime — should need reload
        assert!(watcher.needs_reload(&vault_dir));

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_watcher_needs_reload_returns_true_when_file_newer_than_stored_mtime() {
        let vault_dir = temp_vault_dir();
        let watcher = ConfigWatcherImpl::new();

        // Write initial config and capture its mtime
        let config = AppConfig::default();
        config.save(&vault_dir).unwrap();
        let mut watcher = watcher;
        watcher.update_mtime(&vault_dir);

        // At this point, needs_reload should be false (same mtime)
        assert!(!watcher.needs_reload(&vault_dir));

        // Wait briefly then rewrite the file to get a newer mtime
        std::thread::sleep(std::time::Duration::from_millis(50));
        config.save(&vault_dir).unwrap();

        // Now the file is newer than stored mtime
        assert!(watcher.needs_reload(&vault_dir));

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_watcher_needs_reload_returns_false_when_no_config_file() {
        let vault_dir = temp_vault_dir();
        let watcher = ConfigWatcherImpl::new();

        // No file on disk — needs_reload should return false regardless of stored mtime
        assert!(!watcher.needs_reload(&vault_dir));

        // Also true if we had a previous mtime stored
        let mut watcher = watcher;
        watcher.last_mtime = Some(std::time::SystemTime::now());
        assert!(!watcher.needs_reload(&vault_dir));

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_watcher_needs_reload_returns_true_on_first_time_check() {
        let vault_dir = temp_vault_dir();
        let config = AppConfig::default();
        config.save(&vault_dir).unwrap();

        // Fresh watcher with no stored mtime
        let watcher = ConfigWatcherImpl::new();
        assert!(watcher.needs_reload(&vault_dir));

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_watcher_needs_reload_returns_false_after_update_mtime() {
        let vault_dir = temp_vault_dir();
        let config = AppConfig::default();
        config.save(&vault_dir).unwrap();

        let mut watcher = ConfigWatcherImpl::new();
        assert!(watcher.needs_reload(&vault_dir));

        // After updating mtime to current file mtime, no reload needed
        watcher.update_mtime(&vault_dir);
        assert!(!watcher.needs_reload(&vault_dir));

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_watcher_last_modified_returns_current_file_mtime() {
        let vault_dir = temp_vault_dir();
        let config = AppConfig::default();
        config.save(&vault_dir).unwrap();

        let watcher = ConfigWatcherImpl::new();
        let mtime = watcher.last_modified(&vault_dir);

        assert!(mtime.is_some());

        // The returned mtime should be recent (within last few seconds)
        let elapsed = std::time::SystemTime::now()
            .duration_since(mtime.unwrap())
            .unwrap();
        assert!(elapsed.as_secs() < 5, "mtime should be recent");

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_watcher_last_modified_returns_none_when_no_file() {
        let vault_dir = temp_vault_dir();
        let watcher = ConfigWatcherImpl::new();

        assert!(watcher.last_modified(&vault_dir).is_none());

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_watcher_update_mtime_sets_stored_mtime_to_current_file() {
        let vault_dir = temp_vault_dir();
        let config = AppConfig::default();
        config.save(&vault_dir).unwrap();

        let mut watcher = ConfigWatcherImpl::new();
        assert!(watcher.last_mtime.is_none());

        watcher.update_mtime(&vault_dir);
        assert!(watcher.last_mtime.is_some());

        // The stored mtime should match the file's actual mtime
        let file_mtime = ConfigWatcherImpl::current_mtime(&vault_dir).unwrap();
        assert_eq!(watcher.last_mtime.unwrap(), file_mtime);

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_watcher_update_mtime_with_no_file_clears_stored_mtime() {
        let vault_dir = temp_vault_dir();
        let mut watcher = ConfigWatcherImpl::new();

        // Simulate having a previous mtime
        watcher.last_mtime = Some(std::time::SystemTime::UNIX_EPOCH);

        // update_mtime on a non-existent file should set mtime to None
        watcher.update_mtime(&vault_dir);
        assert!(watcher.last_mtime.is_none());

        cleanup_dir(&vault_dir);
    }

    #[test]
    fn config_watcher_default_is_same_as_new() {
        let via_new = ConfigWatcherImpl::new();
        let via_default = ConfigWatcherImpl::default();
        assert_eq!(via_new.last_mtime, via_default.last_mtime);
    }
}
