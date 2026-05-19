// =============================================================================
// Global Zeroize-on-Crash Registry
// =============================================================================
//
// This module provides a global registry for tracking all locked memory regions
// that contain sensitive data (keys, passwords). The registry is used by crash
// handlers to zeroize all secrets before the process exits.
//
// Key design decisions:
// - Uses spin::Mutex (not std::sync::Mutex) because crash handlers must be
//   async-signal-safe and cannot block
// - zeroize_all() uses try_lock() to avoid blocking in signal context
// - Registry tracks (ptr, len) tuples for raw memory zeroization
//
// This is part of OKI-0006 (macOS Crash Dump Protection Hardening).

#[cfg(unix)]
mod registry {
    use spin::Mutex;

    struct Region {
        ptr: *mut u8,
        len: usize,
    }

    // SAFETY: Region is Send because we only access it from the crash handler
    // which runs in a single-threaded context. The Mutex ensures mutual exclusion
    // for registration/unregistration during normal operation.
    unsafe impl Send for Region {}

    static REGISTRY: Mutex<Vec<Region>> = Mutex::new(Vec::new());

    pub fn register(ptr: *mut u8, len: usize) {
        REGISTRY.lock().push(Region { ptr, len });
    }

    pub fn unregister(ptr: *mut u8) {
        REGISTRY.lock().retain(|r| r.ptr != ptr);
    }

    /// Zeroize all registered secrets. Called from crash handler.
    /// Uses try_lock() to avoid blocking in signal context.
    pub fn zeroize_all() {
        if let Some(registry) = REGISTRY.try_lock() {
            for region in registry.iter() {
                // SAFETY: ptr and len were valid when registered.
                // In crash context, the memory should still be mapped.
                unsafe {
                    core::ptr::write_bytes(region.ptr, 0, region.len);
                    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
                }
            }
        }
    }
}

// Public re-exports
#[cfg(unix)]
pub use registry::{register, unregister, zeroize_all};
