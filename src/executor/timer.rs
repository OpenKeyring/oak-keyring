//! Timer management for auto-sync, auto-lock, and clipboard clear.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::time::{interval, Duration, Interval};

use crate::config::sync::{SyncMode, SyncProvider};
use crate::config::AppConfig;

/// Shared activity tracker for auto-lock idle detection.
///
/// The TUI layer calls [`ActivityTracker::touch`] on every user input event; the executor
/// reads [`ActivityTracker::idle_seconds`] before triggering auto-lock to decide whether the
/// user is still active.
#[derive(Clone)]
pub struct ActivityTracker {
    last_active: Arc<AtomicI64>,
}

impl ActivityTracker {
    pub fn new() -> Self {
        Self {
            last_active: Arc::new(AtomicI64::new(now_secs())),
        }
    }

    /// Record that the user is currently active.
    pub fn touch(&self) {
        self.last_active.store(now_secs(), Ordering::Relaxed);
    }

    /// Seconds elapsed since the last [`ActivityTracker::touch`].
    pub fn idle_seconds(&self) -> i64 {
        now_secs() - self.last_active.load(Ordering::Relaxed)
    }
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Default for ActivityTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ActivityTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivityTracker")
            .field("idle_seconds", &self.idle_seconds())
            .finish()
    }
}

pub struct ExecutorTimers {
    /// Auto-sync interval (None if sync disabled or manual mode)
    pub sync_interval: Option<Interval>,
    /// Auto-lock interval (None if auto_lock_seconds == 0)
    pub auto_lock_interval: Option<Interval>,
    /// Whether auto-sync is active
    pub sync_active: bool,
    /// Whether auto-lock is active
    pub auto_lock_active: bool,
}

impl ExecutorTimers {
    pub fn new(config: &AppConfig, sync_service_available: bool) -> Self {
        let sync_enabled = sync_service_available
            && config.sync.provider != SyncProvider::Disabled
            && config.sync.sync_mode == SyncMode::Auto
            && config.sync.auto_interval_seconds > 0;

        let sync_interval = if sync_enabled {
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

        Self {
            sync_active: sync_interval.is_some(),
            auto_lock_active: auto_lock_interval.is_some(),
            sync_interval,
            auto_lock_interval,
        }
    }

    pub fn reset_auto_lock(&mut self) {
        if let Some(ref mut iv) = self.auto_lock_interval {
            iv.reset();
        }
    }

    pub fn rebuild(&mut self, config: &AppConfig, sync_service_available: bool) {
        *self = Self::new(config, sync_service_available);
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
