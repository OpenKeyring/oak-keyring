//! Timer management for auto-sync, auto-lock, and clipboard clear.

use tokio::time::{interval, Duration, Interval};

use crate::config::AppConfig;

pub struct ExecutorTimers {
    /// Auto-sync interval (None if sync disabled or manual mode)
    pub sync_interval: Option<Interval>,
    /// Auto-lock interval (None if auto_lock_seconds == 0)
    pub auto_lock_interval: Option<Interval>,
    /// Clipboard clear interval
    pub clipboard_clear_interval: Option<Interval>,
    /// Whether auto-sync is active
    pub sync_active: bool,
    /// Whether auto-lock is active
    pub auto_lock_active: bool,
}

impl ExecutorTimers {
    pub fn new(config: &AppConfig) -> Self {
        let sync_interval = if config.sync.auto_interval_seconds > 0 {
            Some(interval(Duration::from_secs(
                config.sync.auto_interval_seconds,
            )))
        } else {
            None
        };

        let auto_lock_interval = if config.general.auto_lock_seconds > 0 {
            Some(interval(Duration::from_secs(
                config.general.auto_lock_seconds,
            )))
        } else {
            None
        };

        let clipboard_clear_interval = if config.general.clipboard_clear_seconds > 0 {
            Some(interval(Duration::from_secs(
                config.general.clipboard_clear_seconds,
            )))
        } else {
            None
        };

        Self {
            sync_active: sync_interval.is_some(),
            auto_lock_active: auto_lock_interval.is_some(),
            sync_interval,
            auto_lock_interval,
            clipboard_clear_interval,
        }
    }

    pub fn reset_auto_lock(&mut self) {
        if let Some(ref mut iv) = self.auto_lock_interval {
            iv.reset();
        }
    }

    pub fn rebuild(&mut self, config: &AppConfig) {
        *self = Self::new(config);
    }
}

/// Await the next tick on an optional interval. If `None`, waits forever (pending).
pub async fn tick_opt(opt: &mut Option<Interval>) {
    match opt {
        Some(ref mut iv) => {
            iv.tick().await;
        }
        None => std::future::pending().await,
    }
}
