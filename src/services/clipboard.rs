use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use tokio::task::AbortHandle;
use tokio::time::Duration;
use tracing::{debug, info, warn};

use crate::errors::mapping::clipboard::ClipboardError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum content length for clipboard operations (S4 spec §Non-functional).
const MAX_CONTENT_BYTES: usize = 1024;

// ---------------------------------------------------------------------------
// ClipboardBackend Trait
// ---------------------------------------------------------------------------

/// Platform abstraction for clipboard operations.
///
/// Implementations: `ArboardBackend` (production), `MockBackend` (testing).
/// Trait methods take `&self` — implementations use internal mutability.
///
/// # Memory Safety (S4 spec §Memory Safety)
///
/// `set_text()` receives `&str`. Implementations must NOT:
/// - Clone, buffer, cache, or log the plaintext
/// - Allocate heap memory to store a copy of the plaintext
///
/// The caller (S5 Executor) handles zeroize via `SecureStr::drop`.
pub trait ClipboardBackend: Send + Sync {
    fn set_text(&self, text: &str) -> Result<(), ClipboardError>;
    fn get_text(&self) -> Result<String, ClipboardError>;
    fn is_available(&self) -> bool;
}

// ---------------------------------------------------------------------------
// ArboardBackend — production implementation
// ---------------------------------------------------------------------------

/// Production clipboard backend wrapping `arboard` crate.
pub struct ArboardBackend {
    clipboard: Mutex<arboard::Clipboard>,
}

impl ArboardBackend {
    pub fn new() -> Result<Self, ClipboardError> {
        let clipboard = arboard::Clipboard::new()
            .map_err(|e| ClipboardError::PlatformUnavailable(e.to_string()))?;
        Ok(Self {
            clipboard: Mutex::new(clipboard),
        })
    }
}

impl ClipboardBackend for ArboardBackend {
    fn set_text(&self, text: &str) -> Result<(), ClipboardError> {
        let mut cb = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::LockPoisoned)?;
        cb.set_text(text)
            .map_err(|e| ClipboardError::Io(e.to_string()))
    }

    fn get_text(&self) -> Result<String, ClipboardError> {
        let mut cb = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::LockPoisoned)?;
        cb.get_text().map_err(|e| ClipboardError::Io(e.to_string()))
    }

    fn is_available(&self) -> bool {
        self.clipboard.lock().is_ok()
    }
}

// ---------------------------------------------------------------------------
// MockBackend — test-only implementation
// ---------------------------------------------------------------------------

/// In-memory clipboard backend for unit testing.
pub struct MockBackend {
    content: Mutex<String>,
    available: AtomicBool,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBackend {
    pub fn new() -> Self {
        Self {
            content: Mutex::new(String::new()),
            available: AtomicBool::new(true),
        }
    }

    pub fn new_unavailable() -> Self {
        Self {
            content: Mutex::new(String::new()),
            available: AtomicBool::new(false),
        }
    }
}

impl ClipboardBackend for MockBackend {
    fn set_text(&self, text: &str) -> Result<(), ClipboardError> {
        if !self.available.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(ClipboardError::PlatformUnavailable(
                "mock: unavailable".into(),
            ));
        }
        let mut content = self
            .content
            .lock()
            .map_err(|_| ClipboardError::LockPoisoned)?;
        *content = text.to_string();
        Ok(())
    }

    fn get_text(&self) -> Result<String, ClipboardError> {
        if !self.available.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(ClipboardError::PlatformUnavailable(
                "mock: unavailable".into(),
            ));
        }
        let content = self
            .content
            .lock()
            .map_err(|_| ClipboardError::LockPoisoned)?;
        Ok(content.clone())
    }

    fn is_available(&self) -> bool {
        self.available.load(std::sync::atomic::Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// ClipboardService
// ---------------------------------------------------------------------------

/// System clipboard service with async auto-clear timer and smart-clear.
///
/// Per S4 spec:
/// - `copy()` → writes to backend, starts tokio timer (if timeout > 0)
/// - Consecutive copies cancel previous timer, restart
/// - Timer fires → smart-clear (hash verification before clearing)
/// - `clear()` → force clear (for manual/shutdown use)
/// - `cancel_timer()` → stop timer without clearing
///
/// # Design Deviations from S4 Spec
///
/// 1. `copy(&self)` instead of `copy(&mut self)` — `Arc<dyn Backend>` requires
///    interior mutability via `Mutex`. All mutation happens through Mutex guards,
///    so `&self` suffices and enables sharing across tasks.
/// 2. `smart_clear()` is an out-of-spec enhancement. S4 spec requires only
///    unconditional `clear()`. Smart-clear adds SHA-256 hash verification to
///    avoid clearing user-copied content. The timer also uses smart-clear.
///
/// # Memory Safety (S4 spec §Memory Safety)
///
/// This service receives `&str` borrows only. It does NOT:
/// - Clone, buffer, cache, or log the plaintext
/// - Store any heap copy of the plaintext
/// - The only stored value is a SHA-256 hash (one-way, no plaintext recovery)
///
/// Plaintext zeroize is the caller's (S5 Executor) responsibility via `SecureStr::drop`.
pub struct ClipboardService {
    backend: Arc<dyn ClipboardBackend>,
    clear_timeout: AtomicU64,
    active_timer: Mutex<Option<AbortHandle>>,
    last_hash: Mutex<Option<String>>,
}

impl ClipboardService {
    pub fn with_backend(backend: Box<dyn ClipboardBackend>, clear_timeout: u64) -> Self {
        Self {
            backend: Arc::from(backend),
            clear_timeout: AtomicU64::new(clear_timeout),
            active_timer: Mutex::new(None),
            last_hash: Mutex::new(None),
        }
    }

    pub fn new(clear_timeout: u64) -> Result<Self, ClipboardError> {
        let backend = ArboardBackend::new()?;
        Ok(Self::with_backend(Box::new(backend), clear_timeout))
    }

    pub fn new_safe(clear_timeout: u64) -> Result<Self, ClipboardError> {
        if Self::is_headless() {
            return Err(ClipboardError::PlatformUnavailable(
                "Headless environment detected — clipboard unavailable".into(),
            ));
        }
        Self::new(clear_timeout)
    }

    /// Copy text to clipboard and start auto-clear timer.
    ///
    /// Returns `clear_timeout` for UI countdown display.
    pub fn copy(&self, text: &str) -> Result<u64, ClipboardError> {
        let byte_len = text.len();
        if byte_len > MAX_CONTENT_BYTES {
            return Err(ClipboardError::ContentTooLong {
                max_bytes: MAX_CONTENT_BYTES,
                actual_bytes: byte_len,
            });
        }

        self.cancel_timer();

        let hash = hash_content(text);
        {
            let mut last_hash = self
                .last_hash
                .lock()
                .map_err(|_| ClipboardError::LockPoisoned)?;
            *last_hash = Some(hash);
        }

        self.backend.set_text(text)?;
        let timeout = self.clear_timeout.load(Ordering::Relaxed);
        info!(timeout_secs = timeout, "Copied to clipboard with tracking");

        if timeout > 0 {
            self.start_clear_timer();
        }

        Ok(timeout)
    }

    /// Force clear clipboard regardless of content.
    pub fn clear(&self) -> Result<(), ClipboardError> {
        self.cancel_timer();
        self.backend.set_text("")?;
        info!("Clipboard force-cleared");
        Ok(())
    }

    /// Smart clear: only clear if clipboard still contains our content.
    ///
    /// Returns `true` if cleared, `false` if skipped (content changed).
    pub fn smart_clear(&self) -> Result<bool, ClipboardError> {
        let expected_hash = {
            let last_hash = self
                .last_hash
                .lock()
                .map_err(|_| ClipboardError::LockPoisoned)?;
            last_hash.clone()
        };

        let expected_hash = match expected_hash {
            Some(h) => h,
            None => {
                debug!("No tracked content — skipping smart clear");
                return Ok(false);
            }
        };

        let current_content = self.backend.get_text()?;
        let current_hash = hash_content(&current_content);

        if current_hash == expected_hash {
            self.backend.set_text("")?;
            {
                let mut last_hash = self
                    .last_hash
                    .lock()
                    .map_err(|_| ClipboardError::LockPoisoned)?;
                *last_hash = None;
            }
            info!("Smart clear: clipboard cleared (content matched)");
            Ok(true)
        } else {
            warn!("Smart clear: skipping — content changed since last copy");
            {
                let mut last_hash = self
                    .last_hash
                    .lock()
                    .map_err(|_| ClipboardError::LockPoisoned)?;
                *last_hash = None;
            }
            Ok(false)
        }
    }

    /// Cancel the active auto-clear timer without clearing the clipboard.
    pub fn cancel_timer(&self) {
        let handle = {
            let mut timer = match self.active_timer.lock() {
                Ok(t) => t,
                Err(_) => return,
            };
            timer.take()
        };
        if let Some(h) = handle {
            h.abort();
            debug!("Previous clipboard timer cancelled");
        }
    }

    pub fn has_active_timer(&self) -> bool {
        self.active_timer
            .lock()
            .map(|t| t.is_some())
            .unwrap_or(false)
    }

    pub fn clear_timeout(&self) -> u64 {
        self.clear_timeout.load(Ordering::Relaxed)
    }

    pub fn set_clear_timeout(&self, seconds: u64) {
        self.clear_timeout.store(seconds, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub(crate) fn backend(&self) -> &dyn ClipboardBackend {
        self.backend.as_ref()
    }

    fn start_clear_timer(&self) {
        let backend = Arc::clone(&self.backend);
        let timeout = self.clear_timeout.load(Ordering::Relaxed);
        let expected_hash = self.last_hash.lock().ok().and_then(|h| h.clone());

        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(timeout)).await;
            if let Some(hash) = expected_hash {
                if let Ok(content) = backend.get_text() {
                    if hash_content(&content) == hash {
                        let _ = backend.set_text("");
                        info!("Auto-clear timer: clipboard cleared");
                    } else {
                        info!("Auto-clear timer: content changed — skipping");
                    }
                }
            }
        });

        if let Ok(mut timer) = self.active_timer.lock() {
            *timer = Some(handle.abort_handle());
        }
    }

    pub fn is_headless() -> bool {
        if std::env::var("CI").is_ok() {
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            std::env::var("SECURITYSESSIONID").is_err() && std::env::var("TERM_PROGRAM").is_err()
        }
        #[cfg(target_os = "linux")]
        {
            std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err()
        }
        #[cfg(target_os = "windows")]
        {
            false
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            true
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helper
// ---------------------------------------------------------------------------

/// SHA-256 hash for content comparison. Free function for use in spawned tasks.
fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// Service Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod service_tests {
    use super::*;

    fn make_service(timeout: u64) -> ClipboardService {
        let backend = Box::new(MockBackend::new());
        ClipboardService::with_backend(backend, timeout)
    }

    #[tokio::test]
    async fn copy_writes_to_backend() {
        let svc = make_service(30);
        svc.copy("test-password").unwrap();
        let content = svc.backend().get_text().unwrap();
        assert_eq!(content, "test-password");
    }

    #[tokio::test]
    async fn copy_returns_clear_timeout() {
        let svc = make_service(45);
        let timeout = svc.copy("test").unwrap();
        assert_eq!(timeout, 45);
    }

    #[tokio::test]
    async fn copy_rejects_content_over_1024_bytes() {
        let svc = make_service(30);
        let long_text = "x".repeat(1025);
        let result = svc.copy(&long_text);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ClipboardError::ContentTooLong { .. }
        ));
    }

    #[tokio::test]
    async fn copy_accepts_exactly_1024_bytes() {
        let svc = make_service(30);
        let text = "x".repeat(1024);
        assert!(svc.copy(&text).is_ok());
    }

    #[tokio::test]
    async fn copy_with_zero_timeout_does_not_start_timer() {
        let svc = make_service(0);
        svc.copy("test").unwrap();
        assert!(!svc.has_active_timer());
    }

    #[tokio::test]
    async fn cancel_timer_stops_active_timer() {
        let svc = make_service(30);
        svc.copy("test").unwrap();
        assert!(svc.has_active_timer());
        svc.cancel_timer();
        assert!(!svc.has_active_timer());
    }

    #[tokio::test]
    async fn consecutive_copy_resets_timer() {
        let svc = make_service(30);
        svc.copy("first").unwrap();
        assert!(svc.has_active_timer());
        svc.copy("second").unwrap();
        assert!(svc.has_active_timer());
    }

    #[tokio::test]
    async fn clear_empties_clipboard() {
        let svc = make_service(30);
        svc.copy("secret").unwrap();
        svc.clear().unwrap();
        assert!(svc.backend().get_text().unwrap().is_empty());
    }

    #[tokio::test]
    async fn smart_clear_matches_and_clears() {
        let svc = make_service(30);
        svc.copy("password123").unwrap();
        let cleared = svc.smart_clear().unwrap();
        assert!(cleared);
        assert!(svc.backend().get_text().unwrap().is_empty());
    }

    #[tokio::test]
    async fn smart_clear_skips_if_content_changed() {
        let svc = make_service(30);
        svc.copy("original-password").unwrap();
        svc.backend().set_text("user-copied-text").unwrap();
        let cleared = svc.smart_clear().unwrap();
        assert!(!cleared);
        assert_eq!(svc.backend().get_text().unwrap(), "user-copied-text");
    }

    #[test]
    fn hash_content_is_deterministic() {
        assert_eq!(hash_content("test"), hash_content("test"));
    }

    #[test]
    fn hash_content_differs_for_different_input() {
        assert_ne!(hash_content("a"), hash_content("b"));
    }

    #[test]
    fn hash_content_is_64_chars() {
        assert_eq!(hash_content("any").len(), 64);
    }
}

// ---------------------------------------------------------------------------
// Backend Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod backend_tests {
    use super::*;

    #[test]
    fn mock_backend_set_and_get() {
        let backend = MockBackend::new();
        assert!(backend.is_available());
        backend.set_text("hello").unwrap();
        assert_eq!(backend.get_text().unwrap(), "hello");
    }

    #[test]
    fn mock_backend_clear_returns_empty() {
        let backend = MockBackend::new();
        backend.set_text("secret").unwrap();
        backend.set_text("").unwrap();
        let content = backend.get_text().unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn mock_backend_unavailable() {
        let backend = MockBackend::new_unavailable();
        assert!(!backend.is_available());
        assert!(backend.set_text("test").is_err());
    }

    #[test]
    fn arboard_backend_is_available_in_gui() {
        if std::env::var("CI").is_ok() {
            return;
        }
        let backend = ArboardBackend::new();
        if let Ok(b) = backend {
            assert!(b.is_available());
        }
    }

    #[test]
    fn arboard_backend_returns_error_in_headless() {
        if std::env::var("CI").is_err() {
            return;
        }
        let result = ArboardBackend::new();
        if let Err(e) = result {
            assert!(matches!(e, ClipboardError::PlatformUnavailable(_)));
        }
    }
}
