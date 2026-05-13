//! Concrete implementations of ConfigManager, ConfigWatcher, and ServiceNotification.
//!
//! These structs implement the D3 config traits for use by the S5 executor layer.

use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use crate::config::{
    AppConfig, ConfigError, ConfigManager, ConfigReloadable, ConfigWatcher, ServiceNotification,
};
use crate::services::clipboard::ClipboardService;

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

    /// Test-only: Directly update the in-memory config.
    /// This bypasses persistence and notification, intended only for test isolation.
    #[cfg(test)]
    pub fn update_config_for_test<F>(&self, updater: F)
    where
        F: FnOnce(&mut AppConfig),
    {
        let mut config = self.config.write().unwrap_or_else(|e| e.into_inner());
        updater(&mut config);
    }
}

impl ConfigManager for ConfigManagerImpl {
    fn load(&self) -> Result<AppConfig, ConfigError> {
        let config = AppConfig::load()?;
        let mut current = self.config.write().unwrap_or_else(|e| e.into_inner());
        *current = config.clone();
        Ok(config)
    }

    fn save(&self, config: &AppConfig) -> Result<(), ConfigError> {
        config.save()?;
        let mut current = self.config.write().unwrap_or_else(|e| e.into_inner());
        *current = config.clone();
        Ok(())
    }

    fn reload(&self) -> Result<AppConfig, ConfigError> {
        self.load()
    }

    fn get_config(&self) -> AppConfig {
        self.config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

// ---------------------------------------------------------------------------
// ConfigWatcherImpl
// ---------------------------------------------------------------------------

/// Polling-based config file change detector using mtime comparison.
pub struct ConfigWatcherImpl {
    pub(crate) last_mtime: Option<SystemTime>,
}

impl ConfigWatcherImpl {
    pub fn new() -> Self {
        Self { last_mtime: None }
    }

    pub(crate) fn current_mtime() -> Option<SystemTime> {
        let config_path = crate::paths::config_file_path();
        std::fs::metadata(&config_path).ok()?.modified().ok()
    }
}

impl Default for ConfigWatcherImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigWatcher for ConfigWatcherImpl {
    fn needs_reload(&self) -> bool {
        let current = Self::current_mtime();
        match (current, self.last_mtime) {
            (Some(now), Some(last)) => now > last,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    fn last_modified(&self) -> Option<SystemTime> {
        Self::current_mtime()
    }
}

// Note: ConfigWatcherImpl::needs_reload is read-only (&self) per the trait.
// To update the stored mtime after a reload, callers should drop and recreate
// the watcher, or we provide a separate update method.

impl ConfigWatcherImpl {
    /// Update the stored mtime snapshot to the current file mtime.
    /// Call this after a successful reload to avoid repeated reload triggers.
    pub fn update_mtime(&mut self) {
        self.last_mtime = Self::current_mtime();
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

// ---------------------------------------------------------------------------
// ClipboardConfigAdapter
// ---------------------------------------------------------------------------

/// Adapter that bridges `ClipboardService` into the `ConfigReloadable` notification system.
///
/// Holds a shared reference (`Arc`) to the clipboard service and delegates
/// `reload()` calls to `set_clear_timeout()`.
pub struct ClipboardConfigAdapter {
    inner: Arc<ClipboardService>,
}

impl ClipboardConfigAdapter {
    pub fn new(clipboard: Arc<ClipboardService>) -> Self {
        Self { inner: clipboard }
    }
}

impl ConfigReloadable for ClipboardConfigAdapter {
    fn service_id(&self) -> &str {
        "clipboard"
    }

    fn reload(&self, config: &AppConfig) -> Result<(), ConfigError> {
        self.inner
            .set_clear_timeout(config.general.clipboard_clear_seconds);
        tracing::info!(
            timeout = config.general.clipboard_clear_seconds,
            "ClipboardService reloaded via config notification"
        );
        Ok(())
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
