//! Snapshot-test locale and timezone guard.
//!
//! Every snapshot test renders under a fixed locale ("en" by default) and a
//! fixed timezone (UTC) so that generated `.snap` files are deterministic
//! regardless of the developer machine's `LANG` / `LC_ALL` / `TZ` settings.
//!
//! Usage:
//! ```ignore
//! let _locale = snapshot_locale();
//! // … render screen …
//! ```

use std::sync::{Mutex, MutexGuard};

/// Process-wide mutex that serialises all locale-sensitive snapshot renders.
static LOCALE_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard: sets `rust_i18n` locale and `TZ` to fixed values for the
/// duration of rendering, then restores the previous values on drop.
pub struct SnapshotLocaleGuard {
    original_locale: String,
    original_tz: Option<String>,
    _lock: MutexGuard<'static, ()>,
}

impl SnapshotLocaleGuard {
    /// Acquire the locale lock and switch to the given locale and UTC timezone.
    pub fn new(locale: &str) -> Self {
        let lock = LOCALE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let original_locale = rust_i18n::locale().to_string();
        rust_i18n::set_locale(locale);

        let original_tz = std::env::var("TZ").ok();
        std::env::set_var("TZ", "UTC");

        Self {
            original_locale,
            original_tz,
            _lock: lock,
        }
    }
}

impl Drop for SnapshotLocaleGuard {
    fn drop(&mut self) {
        rust_i18n::set_locale(&self.original_locale);
        match &self.original_tz {
            Some(tz) => std::env::set_var("TZ", tz),
            None => std::env::remove_var("TZ"),
        }
    }
}

/// Convenience: acquire locale guard pinned to English with UTC timezone.
pub fn snapshot_locale() -> SnapshotLocaleGuard {
    SnapshotLocaleGuard::new("en")
}
