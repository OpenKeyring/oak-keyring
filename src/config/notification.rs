//! ServiceNotification trait — service config-change notification interface.
//!
//! Implementation lives in Plan K (S5 Executor).

use crate::config::AppConfig;
use crate::config::ConfigError;

/// Service config-change notification interface.
///
/// Defines how services are notified after a configuration change.
/// The implementation (Plan K) holds references to services and calls their reload methods.
pub trait ServiceNotification: Send + Sync {
    /// Notify services that configuration has changed.
    ///
    /// `config` is the new configuration to reload into each service.
    /// `changed_fields` lists service IDs that should be notified.
    /// An empty list means a full reload (notify all registered services).
    /// Returns one result per notified service.
    fn notify_config_change(
        &self,
        config: &AppConfig,
        changed_fields: &[&str],
    ) -> Vec<Result<(), ConfigError>>;

    /// Register a service to receive config-change notifications.
    fn register_service(&mut self, service: Box<dyn ConfigReloadable>);

    /// Unregister a service by its ID.
    fn unregister_service(&mut self, service_id: &str);
}

/// Interface for services that can reload their configuration.
pub trait ConfigReloadable: Send + Sync {
    /// Unique identifier for the service.
    fn service_id(&self) -> &str;

    /// Reload the service with the given configuration.
    fn reload(&self, config: &crate::config::AppConfig) -> Result<(), ConfigError>;
}
