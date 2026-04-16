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
}
