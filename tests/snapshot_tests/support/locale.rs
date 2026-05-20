//! Snapshot-test locale guard.
//!
//! Every snapshot test renders under a fixed locale ("en" by default) so that
//! generated `.snap` files are deterministic regardless of the developer
//! machine's `LANG` / `LC_ALL` / system-locale setting.
//!
//! Usage:
//! ```ignore
//! let _locale = snapshot_locale();
//! // … render screen …
//! ```

use std::sync::{Mutex, MutexGuard};

/// Process-wide mutex that serialises all locale-sensitive snapshot renders.
static LOCALE_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard: sets `rust_i18n` locale to a fixed value for the duration of
/// rendering, then restores the previous locale on drop.
pub struct SnapshotLocaleGuard {
    original: String,
    _lock: MutexGuard<'static, ()>,
}

impl SnapshotLocaleGuard {
    /// Acquire the locale lock and switch to the given locale.
    pub fn new(locale: &str) -> Self {
        let lock = LOCALE_LOCK.lock().unwrap();
        let original = rust_i18n::locale().to_string();
        rust_i18n::set_locale(locale);
        Self {
            original,
            _lock: lock,
        }
    }
}

impl Drop for SnapshotLocaleGuard {
    fn drop(&mut self) {
        rust_i18n::set_locale(&self.original);
    }
}

/// Convenience: acquire locale guard pinned to English.
pub fn snapshot_locale() -> SnapshotLocaleGuard {
    SnapshotLocaleGuard::new("en")
}
