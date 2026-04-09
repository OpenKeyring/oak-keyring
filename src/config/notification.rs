//! ServiceNotification trait — 服务配置变更通知接口
//!
//! 实现位于 Plan K (S5 Executor)。

use crate::config::ConfigError;

/// 服务配置变更通知接口
///
/// 定义当配置变更后如何通知各个 Service。
/// 实现类（Plan K）持有各 Service 的引用并调用其 reload/update 方法。
pub trait ServiceNotification: Send + Sync {
    /// 通知各 Service 配置已变更
    ///
    /// changed_fields 列出变更的配置字段名，用于 Service 判断是否需要响应。
    /// 返回每个 Service 的通知结果。
    fn notify_config_change(&self, changed_fields: &[&str]) -> Vec<Result<(), ConfigError>>;

    /// 注册需要接收配置变更通知的 Service
    fn register_service(&mut self, service: Box<dyn ConfigReloadable>);

    /// 取消注册 Service
    fn unregister_service(&mut self, service_id: &str);
}

/// 可配置重载的 Service 接口
pub trait ConfigReloadable: Send + Sync {
    /// Service 唯一标识
    fn service_id(&self) -> &str;

    /// 重新加载配置
    fn reload(&self, config: &crate::config::AppConfig) -> Result<(), ConfigError>;
}
