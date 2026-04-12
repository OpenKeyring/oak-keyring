use std::sync::{atomic::AtomicBool, Mutex};

use sha2::{Digest, Sha256};

use crate::errors::mapping::clipboard::ClipboardError;

#[allow(dead_code)]
const MAX_CONTENT_BYTES: usize = 1024;

pub trait ClipboardBackend: Send + Sync {
    fn set_text(&self, text: &str) -> Result<(), ClipboardError>;
    fn get_text(&self) -> Result<String, ClipboardError>;
    fn is_available(&self) -> bool;
}

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

pub struct ClipboardService;

#[allow(dead_code)]
fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

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
        assert!(result.is_err());
    }
}
