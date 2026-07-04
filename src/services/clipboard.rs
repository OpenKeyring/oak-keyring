use std::io::Write;
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use tokio::task::AbortHandle;
use tokio::time::Duration;
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

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
/// - Cache, retain, or log the plaintext
/// - Put the plaintext in process arguments or shell command strings
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
// CommandClipboardBackend — fallback for platform clipboard commands
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPlatform {
    Macos,
    Linux,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClipboardCommandSpec {
    program: String,
    args: Vec<String>,
}

impl ClipboardCommandSpec {
    fn without_args(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    fn new(program: impl Into<String>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommandBackendPlan {
    name: &'static str,
    copy: ClipboardCommandSpec,
    read: ClipboardCommandSpec,
    clear: ClipboardCommandSpec,
}

impl CommandBackendPlan {
    fn required_programs(&self) -> [&str; 3] {
        [
            self.copy.program.as_str(),
            self.read.program.as_str(),
            self.clear.program.as_str(),
        ]
    }
}

struct CommandClipboardBackend {
    plan: CommandBackendPlan,
}

impl CommandClipboardBackend {
    fn new() -> Result<Self, ClipboardError> {
        let Some(path) = std::env::var_os("PATH") else {
            return Err(ClipboardError::PlatformUnavailable(
                "PATH is not set for clipboard command fallback".into(),
            ));
        };
        command_backend_from_search_path(current_command_platform(), &path).ok_or_else(|| {
            ClipboardError::PlatformUnavailable(
                "No complete clipboard command backend found".into(),
            )
        })
    }

    fn with_plan(plan: CommandBackendPlan) -> Self {
        Self { plan }
    }

    fn plan_name(&self) -> &'static str {
        self.plan.name
    }

    fn run_with_stdin(spec: &ClipboardCommandSpec, input: &str) -> Result<(), ClipboardError> {
        let mut child = ProcessCommand::new(&spec.program)
            .args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| command_error(spec, "spawn", e))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| ClipboardError::Io(format!("{} stdin unavailable", spec.program)))?;
        stdin
            .write_all(input.as_bytes())
            .map_err(|e| command_error(spec, "write stdin", e))?;
        drop(stdin);

        let status = child.wait().map_err(|e| command_error(spec, "wait", e))?;
        if status.success() {
            Ok(())
        } else {
            Err(ClipboardError::Io(format!(
                "{} exited with status {}",
                spec.program, status
            )))
        }
    }

    fn capture_stdout(spec: &ClipboardCommandSpec) -> Result<String, ClipboardError> {
        let output = ProcessCommand::new(&spec.program)
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| command_error(spec, "capture stdout", e))?;

        if !output.status.success() {
            return Err(ClipboardError::Io(format!(
                "{} exited with status {}",
                spec.program, output.status
            )));
        }

        String::from_utf8(output.stdout).map_err(|_| {
            ClipboardError::Io(format!(
                "{} returned non-UTF-8 clipboard text",
                spec.program
            ))
        })
    }
}

impl ClipboardBackend for CommandClipboardBackend {
    fn set_text(&self, text: &str) -> Result<(), ClipboardError> {
        if text.is_empty() {
            Self::run_with_stdin(&self.plan.clear, "")
        } else {
            Self::run_with_stdin(&self.plan.copy, text)
        }
    }

    fn get_text(&self) -> Result<String, ClipboardError> {
        Self::capture_stdout(&self.plan.read)
    }

    fn is_available(&self) -> bool {
        true
    }
}

fn current_command_platform() -> CommandPlatform {
    #[cfg(target_os = "macos")]
    {
        return CommandPlatform::Macos;
    }
    #[cfg(target_os = "linux")]
    {
        return CommandPlatform::Linux;
    }
    #[allow(unreachable_code)]
    CommandPlatform::Unsupported
}

fn command_backend_plans_for(platform: CommandPlatform) -> Vec<CommandBackendPlan> {
    match platform {
        CommandPlatform::Macos => vec![CommandBackendPlan {
            name: "pbcopy",
            copy: ClipboardCommandSpec::without_args("pbcopy"),
            read: ClipboardCommandSpec::without_args("pbpaste"),
            clear: ClipboardCommandSpec::without_args("pbcopy"),
        }],
        CommandPlatform::Linux => vec![
            CommandBackendPlan {
                name: "wl-copy",
                copy: ClipboardCommandSpec::new("wl-copy", ["--type", "text/plain;charset=utf-8"]),
                read: ClipboardCommandSpec::without_args("wl-paste"),
                clear: ClipboardCommandSpec::new("wl-copy", ["--clear"]),
            },
            CommandBackendPlan {
                name: "xclip",
                copy: ClipboardCommandSpec::new("xclip", ["-selection", "clipboard", "-in"]),
                read: ClipboardCommandSpec::new("xclip", ["-selection", "clipboard", "-out"]),
                clear: ClipboardCommandSpec::new("xclip", ["-selection", "clipboard", "-in"]),
            },
            CommandBackendPlan {
                name: "xsel",
                copy: ClipboardCommandSpec::new("xsel", ["--clipboard", "--input"]),
                read: ClipboardCommandSpec::new("xsel", ["--clipboard", "--output"]),
                clear: ClipboardCommandSpec::new("xsel", ["--clipboard", "--delete"]),
            },
        ],
        CommandPlatform::Unsupported => Vec::new(),
    }
}

#[cfg(test)]
fn command_backend_from_path(
    platform: CommandPlatform,
    path: &std::path::Path,
) -> Option<CommandClipboardBackend> {
    command_backend_from_search_path(platform, path.as_os_str())
}

fn command_backend_from_search_path(
    platform: CommandPlatform,
    search_path: &std::ffi::OsStr,
) -> Option<CommandClipboardBackend> {
    command_backend_plans_for(platform)
        .into_iter()
        .find(|plan| {
            plan.required_programs()
                .into_iter()
                .all(|program| executable_in_path(program, search_path))
        })
        .map(CommandClipboardBackend::with_plan)
}

fn executable_in_path(program: &str, search_path: &std::ffi::OsStr) -> bool {
    std::env::split_paths(search_path).any(|dir| is_executable(&dir.join(program)))
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

fn command_error(
    spec: &ClipboardCommandSpec,
    action: &'static str,
    error: std::io::Error,
) -> ClipboardError {
    ClipboardError::Io(format!(
        "failed to {} for clipboard command {}: {}",
        action, spec.program, error
    ))
}

/// Backend used when the platform clipboard is unavailable.
struct UnavailableBackend {
    reason: String,
}

impl UnavailableBackend {
    fn new(reason: String) -> Self {
        Self { reason }
    }
}

impl ClipboardBackend for UnavailableBackend {
    fn set_text(&self, _text: &str) -> Result<(), ClipboardError> {
        Err(ClipboardError::PlatformUnavailable(self.reason.clone()))
    }

    fn get_text(&self) -> Result<String, ClipboardError> {
        Err(ClipboardError::PlatformUnavailable(self.reason.clone()))
    }

    fn is_available(&self) -> bool {
        false
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
// Clipboard Trait
// ---------------------------------------------------------------------------

/// Executor-facing clipboard capability.
///
/// High-level clipboard operations for copy/clear with auto-clear timer support.
#[cfg_attr(test, mockall::automock)]
pub trait Clipboard: Send + Sync {
    fn copy(&self, text: &str) -> Result<u64, ClipboardError>;
    fn clear(&self) -> Result<(), ClipboardError>;
    fn smart_clear(&self) -> Result<bool, ClipboardError>;
    fn set_clear_timeout(&self, seconds: u64);
    fn clear_timeout(&self) -> u64;
    fn cancel_timer(&self);
}

// ---------------------------------------------------------------------------
// ClipboardServiceImpl
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
pub struct ClipboardServiceImpl {
    backend: Arc<dyn ClipboardBackend>,
    clear_timeout: AtomicU64,
    active_timer: Mutex<Option<AbortHandle>>,
    last_hash: Mutex<Option<String>>,
}

/// Type alias for backward compatibility.
pub type ClipboardService = ClipboardServiceImpl;

impl ClipboardServiceImpl {
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
            if let Ok(backend) = CommandClipboardBackend::new() {
                info!(
                    backend = backend.plan_name(),
                    "Using command clipboard backend in headless environment"
                );
                return Ok(Self::with_backend(Box::new(backend), clear_timeout));
            }

            return Ok(Self::with_backend(
                Box::new(UnavailableBackend::new(
                    "Headless environment detected — clipboard unavailable".into(),
                )),
                clear_timeout,
            ));
        }

        match ArboardBackend::new() {
            Ok(backend) => Ok(Self::with_backend(Box::new(backend), clear_timeout)),
            Err(ClipboardError::PlatformUnavailable(reason)) => {
                if let Ok(backend) = CommandClipboardBackend::new() {
                    info!(
                        reason,
                        backend = backend.plan_name(),
                        "System clipboard unavailable, using command clipboard backend"
                    );
                    return Ok(Self::with_backend(Box::new(backend), clear_timeout));
                }

                warn!(
                    reason,
                    "System clipboard unavailable, falling back to disabled backend"
                );
                Ok(Self::with_backend(
                    Box::new(UnavailableBackend::new(reason)),
                    clear_timeout,
                ))
            }
            Err(err) => Err(err),
        }
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

        let current_content = Zeroizing::new(self.backend.get_text()?);
        let current_hash = hash_content(current_content.as_str());

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
        let has_tracked = self.last_hash.lock().map(|h| h.is_some()).unwrap_or(false);
        if has_tracked {
            self.cancel_timer();
            if seconds > 0 {
                self.start_clear_timer();
            }
        }
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
                    let content = Zeroizing::new(content);
                    if hash_content(content.as_str()) == hash {
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

impl Clipboard for ClipboardServiceImpl {
    fn copy(&self, text: &str) -> Result<u64, ClipboardError> {
        // Delegate to inherent impl
        let byte_len = text.len();
        if byte_len > MAX_CONTENT_BYTES {
            return Err(ClipboardError::ContentTooLong {
                max_bytes: MAX_CONTENT_BYTES,
                actual_bytes: byte_len,
            });
        }

        // Call cancel_timer from inherent impl
        let handle = {
            let mut timer = match self.active_timer.lock() {
                Ok(t) => t,
                Err(_) => return Err(ClipboardError::LockPoisoned),
            };
            timer.take()
        };
        if let Some(h) = handle {
            h.abort();
            debug!("Previous clipboard timer cancelled");
        }

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

    fn clear(&self) -> Result<(), ClipboardError> {
        // Cancel timer then clear
        let handle = {
            let mut timer = match self.active_timer.lock() {
                Ok(t) => t,
                Err(_) => return Err(ClipboardError::LockPoisoned),
            };
            timer.take()
        };
        if let Some(h) = handle {
            h.abort();
        }

        self.backend.set_text("")?;
        info!("Clipboard force-cleared");
        Ok(())
    }

    fn smart_clear(&self) -> Result<bool, ClipboardError> {
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

        let current_content = Zeroizing::new(self.backend.get_text()?);
        let current_hash = hash_content(current_content.as_str());

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

    fn set_clear_timeout(&self, seconds: u64) {
        self.clear_timeout.store(seconds, Ordering::Relaxed);
        let has_tracked = self.last_hash.lock().map(|h| h.is_some()).unwrap_or(false);
        if has_tracked {
            // Cancel timer
            let handle = {
                let mut timer = match self.active_timer.lock() {
                    Ok(t) => t,
                    Err(_) => return,
                };
                timer.take()
            };
            if let Some(h) = handle {
                h.abort();
            }
            if seconds > 0 {
                self.start_clear_timer();
            }
        }
    }

    fn clear_timeout(&self) -> u64 {
        self.clear_timeout.load(Ordering::Relaxed)
    }

    fn cancel_timer(&self) {
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

    fn make_service(timeout: u64) -> ClipboardServiceImpl {
        let backend = Box::new(MockBackend::new());
        ClipboardServiceImpl::with_backend(backend, timeout)
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
    fn command_backend_plans_match_clipboard_research() {
        let macos = command_backend_plans_for(CommandPlatform::Macos);
        assert_eq!(macos.len(), 1);
        assert_eq!(macos[0].name, "pbcopy");
        assert_eq!(macos[0].copy.program, "pbcopy");
        assert!(macos[0].copy.args.is_empty());
        assert_eq!(macos[0].read.program, "pbpaste");
        assert!(macos[0].read.args.is_empty());
        assert_eq!(macos[0].clear.program, "pbcopy");
        assert!(macos[0].clear.args.is_empty());

        let linux = command_backend_plans_for(CommandPlatform::Linux);
        let names: Vec<_> = linux.iter().map(|plan| plan.name).collect();
        assert_eq!(names, vec!["wl-copy", "xclip", "xsel"]);
        assert_eq!(linux[0].copy.program, "wl-copy");
        assert_eq!(linux[0].copy.args, ["--type", "text/plain;charset=utf-8"]);
        assert_eq!(linux[0].read.program, "wl-paste");
        assert_eq!(linux[0].clear.program, "wl-copy");
        assert_eq!(linux[0].clear.args, ["--clear"]);
        assert_eq!(linux[1].copy.args, ["-selection", "clipboard", "-in"]);
        assert_eq!(linux[1].read.args, ["-selection", "clipboard", "-out"]);
        assert_eq!(linux[2].copy.args, ["--clipboard", "--input"]);
        assert_eq!(linux[2].read.args, ["--clipboard", "--output"]);
        assert_eq!(linux[2].clear.args, ["--clipboard", "--delete"]);
    }

    #[cfg(unix)]
    #[test]
    fn command_backend_writes_secret_via_stdin_not_argv() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = temp.path().join("fake-copy");
        let stdin_path = temp.path().join("stdin.txt");
        let argv_path = temp.path().join("argv.txt");
        write_executable(
            &script,
            &format!(
                "#!/bin/sh\ncat > '{}'\nprintf '%s\\n' \"$@\" > '{}'\n",
                stdin_path.display(),
                argv_path.display()
            ),
        );

        let plan = CommandBackendPlan {
            name: "fake-copy",
            copy: ClipboardCommandSpec::without_args(script.to_string_lossy()),
            read: ClipboardCommandSpec::without_args(script.to_string_lossy()),
            clear: ClipboardCommandSpec::without_args(script.to_string_lossy()),
        };
        let backend = CommandClipboardBackend::with_plan(plan);

        backend.set_text("secret-from-test").expect("copy");

        assert_eq!(
            std::fs::read_to_string(stdin_path).expect("stdin file"),
            "secret-from-test"
        );
        assert!(!std::fs::read_to_string(argv_path)
            .expect("argv file")
            .contains("secret-from-test"));
    }

    #[cfg(unix)]
    #[test]
    fn command_backend_selection_requires_complete_plan_and_prefers_wayland() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_noop_executable(&temp.path().join("wl-copy"));
        write_noop_executable(&temp.path().join("xclip"));

        let backend = command_backend_from_path(CommandPlatform::Linux, temp.path())
            .expect("xclip should be selected when wl-paste is missing");
        assert_eq!(backend.plan_name(), "xclip");

        write_noop_executable(&temp.path().join("wl-paste"));
        let backend = command_backend_from_path(CommandPlatform::Linux, temp.path())
            .expect("wl-copy should be selected once wl-paste is present");
        assert_eq!(backend.plan_name(), "wl-copy");
    }

    #[cfg(unix)]
    #[test]
    fn command_backend_copy_read_and_clear_use_configured_commands() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = temp.path().join("clipboard.txt");
        let copy = temp.path().join("fake-copy");
        let read = temp.path().join("fake-read");
        let clear = temp.path().join("fake-clear");
        write_executable(&copy, &format!("#!/bin/sh\ncat > '{}'\n", store.display()));
        write_executable(&read, &format!("#!/bin/sh\ncat '{}'\n", store.display()));
        write_executable(&clear, &format!("#!/bin/sh\n: > '{}'\n", store.display()));

        let plan = CommandBackendPlan {
            name: "fake",
            copy: ClipboardCommandSpec::without_args(copy.to_string_lossy()),
            read: ClipboardCommandSpec::without_args(read.to_string_lossy()),
            clear: ClipboardCommandSpec::without_args(clear.to_string_lossy()),
        };
        let backend = CommandClipboardBackend::with_plan(plan);

        backend.set_text("secret").expect("copy");
        assert_eq!(backend.get_text().expect("read"), "secret");
        backend.set_text("").expect("clear");
        assert_eq!(backend.get_text().expect("read after clear"), "");
    }

    #[cfg(unix)]
    fn write_noop_executable(path: &std::path::Path) {
        write_executable(path, "#!/bin/sh\nexit 0\n");
    }

    #[cfg(unix)]
    fn write_executable(path: &std::path::Path, content: &str) {
        std::fs::write(path, content).expect("write executable");
        let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o700);
        std::fs::set_permissions(path, permissions).expect("set executable permissions");
    }

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
    fn unavailable_backend_always_reports_platform_unavailable() {
        let backend = UnavailableBackend::new("no clipboard".into());
        assert!(!backend.is_available());
        assert!(matches!(
            backend.set_text("test"),
            Err(ClipboardError::PlatformUnavailable(_))
        ));
        assert!(matches!(
            backend.get_text(),
            Err(ClipboardError::PlatformUnavailable(_))
        ));
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
