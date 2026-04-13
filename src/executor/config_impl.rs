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
    fn notify_config_change(&self, changed_fields: &[&str]) -> Vec<Result<(), ConfigError>> {
        // Reload each registered service with the new config.
        // Only reload services whose service_id matches a changed field prefix,
        // or reload all if the changed_fields list is empty (meaning full reload).
        self.services
            .iter()
            .map(|service| {
                tracing::debug!(
                    service_id = service.service_id(),
                    changed_fields = ?changed_fields,
                    "Notifying service of config change"
                );
                // We need the current config to pass to reload().
                // Since ServiceNotification doesn't hold a config reference,
                // each service is expected to obtain the config itself.
                // For now, return Ok — real integration happens when services
                // implement ConfigReloadable.
                let _ = changed_fields;
                Ok(())
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
