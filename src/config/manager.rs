//! ConfigManager trait — 配置管理层接口
//!
//! 实现位于 Plan K (S5 Executor)。

use crate::config::{AppConfig, ConfigError};
use std::path::Path;

/// 配置管理器接口
///
/// 定义配置加载、保存、重新加载的行为。
/// 实现类（Plan K）持有 AppConfig 实例并响应配置变更。
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
