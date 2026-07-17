//! ConfigWatcher trait — config change detection interface
//!
//! Implementation lives in Plan K (S5 Executor).
//!
//! Note: this interface is for manual config change detection (not filesystem hot reload),
//! determining whether the config file was externally modified by comparing mtime.

/// Config change detector interface
///
/// Used to detect whether the config file was externally modified after the in-memory version.
/// Does not implement filesystem watch (D3 spec explicitly excludes hot reload).
pub trait ConfigWatcher: Send + Sync {
    /// Check whether the config file was modified after the in-memory version
    ///
    /// Returns true to indicate a reload is needed
    fn needs_reload(&self) -> bool;

    /// Get the last modification time of the config file
    fn last_modified(&self) -> Option<std::time::SystemTime>;
}
