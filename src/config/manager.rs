//! ConfigManager trait — configuration management layer interface
//!
//! Implementation lives in Plan K (S5 Executor).

use crate::config::{AppConfig, ConfigError};
use std::path::Path;

/// Configuration manager interface
///
/// Defines behavior for loading, saving, and reloading configuration.
/// Implementations (Plan K) hold an AppConfig instance and respond to configuration changes.
pub trait ConfigManager: Send + Sync {
    /// Load configuration from the specified config directory
    fn load(&self, config_dir: &Path) -> Result<AppConfig, ConfigError>;

    /// Save configuration to the specified config directory
    fn save(&self, config: &AppConfig, config_dir: &Path) -> Result<(), ConfigError>;

    /// Reload configuration from the specified config directory (called after detecting changes)
    fn reload(&self, config_dir: &Path) -> Result<AppConfig, ConfigError>;

    /// Get current in-memory configuration snapshot
    fn get_config(&self) -> AppConfig;
}
