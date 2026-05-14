//! ConfigWatcher trait — 配置变更检测接口
//!
//! 实现位于 Plan K (S5 Executor)。
//!
//! 注意：此接口用于手动检测配置变更（非文件系统热重载），
//! 通过 mtime 比对判断配置文件是否被外部修改。

/// 配置变更检测器接口
///
/// 用于检测配置文件在内存中的版本之后是否被外部修改。
/// 不实现文件系统 watch（D3 spec 明确排除热重载）。
pub trait ConfigWatcher: Send + Sync {
    /// 检查配置文件是否在内存中的版本之后被修改
    ///
    /// 返回 true 表示需要重新加载
    fn needs_reload(&self) -> bool;

    /// 获取配置文件的上次修改时间
    fn last_modified(&self) -> Option<std::time::SystemTime>;
}
