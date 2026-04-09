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
    /// 从指定目录加载配置
    fn load(&self, vault_dir: &Path) -> Result<AppConfig, ConfigError>;

    /// 保存配置到指定目录
    fn save(&self, config: &AppConfig, vault_dir: &Path) -> Result<(), ConfigError>;

    /// 重新加载配置（检测到变更后调用）
    fn reload(&self, vault_dir: &Path) -> Result<AppConfig, ConfigError>;

    /// 获取当前内存中的配置快照
    fn get_config(&self) -> AppConfig;
}
