//! ConfigManager trait — 配置管理层接口
//!
//! 实现位于 Plan K (S5 Executor)。

use crate::config::{AppConfig, ConfigError};

/// 配置管理器接口
///
/// 定义配置加载、保存、重新加载的行为。
/// 实现类（Plan K）持有 AppConfig 实例并响应配置变更。
pub trait ConfigManager: Send + Sync {
    /// Load configuration from the default location
    fn load(&self) -> Result<AppConfig, ConfigError>;

    /// Save configuration to the default location
    fn save(&self, config: &AppConfig) -> Result<(), ConfigError>;

    /// Reload configuration (called after detecting changes)
    fn reload(&self) -> Result<AppConfig, ConfigError>;

    /// Get current in-memory configuration snapshot
    fn get_config(&self) -> AppConfig;
}
