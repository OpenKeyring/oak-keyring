// =============================================================================
// Locked Memory Primitives
// =============================================================================
//
// This module provides memory locking primitives to prevent sensitive data
// (encryption keys, passwords) from being swapped to disk.
//
// Platform-specific implementation:
// - Unix (macOS, Linux): uses mlock()/munlock() via libc
// - Windows: uses VirtualLock()/VirtualUnlock() via windows-sys
//
// Safety considerations:
// - LockedRegion stores raw pointers - NOT Clone/Copy
// - Drop must zeroize before unlocking
// - LockedSecretBytes and LockedKey32 are NOT Clone (prevent key duplication)
// - Debug shows "***REDACTED***" to prevent leaks through logs

use std::fmt;

// =============================================================================
// Platform-specific memory locking
// =============================================================================

#[cfg(unix)]
fn lock_memory_region(ptr: *mut u8, len: usize) -> Result<(), String> {
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
    let base = (ptr as usize) & !(page_size - 1);
    let end = ((ptr as usize) + len + page_size - 1) & !(page_size - 1);
    let aligned_len = end - base;

    let result = unsafe { libc::mlock(base as *const libc::c_void, aligned_len) };
    if result != 0 {
        return Err(format!(
            "mlock failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn unlock_memory_region(ptr: *mut u8, len: usize) {
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
    let base = (ptr as usize) & !(page_size - 1);
    let end = ((ptr as usize) + len + page_size - 1) & !(page_size - 1);
    let aligned_len = end - base;
    unsafe {
        libc::munlock(base as *const libc::c_void, aligned_len);
    }
}

#[cfg(windows)]
fn lock_memory_region(ptr: *mut u8, len: usize) -> Result<(), String> {
    use windows_sys::Win32::System::Memory::VirtualLock;

    let page_size = unsafe {
        let mut info = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetSystemInfo(&mut info);
        info.dwPageSize as usize
    };

    let base = (ptr as usize) & !(page_size - 1);
    let end = ((ptr as usize) + len + page_size - 1) & !(page_size - 1);
    let aligned_len = end - base;

    let result = unsafe { VirtualLock(base as *const _, aligned_len) };
    if result == 0 {
        return Err(format!(
            "VirtualLock failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn unlock_memory_region(ptr: *mut u8, len: usize) {
    use windows_sys::Win32::System::Memory::VirtualUnlock;

    let page_size = unsafe {
        let mut info = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetSystemInfo(&mut info);
        info.dwPageSize as usize
    };

    let base = (ptr as usize) & !(page_size - 1);
    let end = ((ptr as usize) + len + page_size - 1) & !(page_size - 1);
    let aligned_len = end - base;

    unsafe {
        VirtualUnlock(base as *const _, aligned_len);
    }
}

// =============================================================================
// LockedRegion
// =============================================================================

/// A locked region of memory.
///
/// This type is intentionally not Clone or Copy to prevent duplication
/// of the raw pointer, which could lead to double-unlock.
pub struct LockedRegion {
    base: *mut u8,
    len: usize,
}

// SAFETY: LockedRegion is platform-specific and stores raw pointers.
// We don't implement Send/Sync manually - they are derived based on the
// platform's mlock/VirtualLock semantics. On Unix, mlock is thread-safe,
// and on Windows, VirtualLock is also thread-safe.

// =============================================================================
// LockedSecretBytes
// =============================================================================

/// A byte buffer stored in locked memory pages.
///
/// This type ensures that sensitive data is:
/// 1. Locked in memory (prevented from being swapped to disk)
/// 2. Zeroized on drop (securely erased)
/// 3. Not cloneable (prevents accidental duplication)
///
/// # Examples
///
/// ```
/// use oak_keyring::security::LockedSecretBytes;
///
/// let bytes = LockedSecretBytes::with_len(32).unwrap();
/// // ... use the bytes ...
/// // When dropped, the memory is zeroized and unlocked
/// ```
pub struct LockedSecretBytes {
    bytes: Vec<u8>,
    locked_region: Option<LockedRegion>,
}

impl LockedSecretBytes {
    /// Creates a new locked byte buffer with the specified length.
    ///
    /// A length of 0 is allowed and creates an empty buffer without locking.
    ///
    /// # Errors
    ///
    /// Returns an error if the memory lock fails (e.g., OS resource limits).
    pub fn with_len(len: usize) -> Result<Self, String> {
        let mut bytes = vec![0u8; len];
        let locked_region = if len > 0 {
            let ptr = bytes.as_mut_ptr();
            lock_memory_region(ptr, len)?;
            Some(LockedRegion { base: ptr, len })
        } else {
            None
        };
        Ok(Self { bytes, locked_region })
    }

    /// Exposes the underlying bytes as a slice.
    pub fn expose(&self) -> &[u8] {
        &self.bytes
    }

    /// Exposes the underlying bytes as a mutable slice.
    pub fn expose_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}

impl Drop for LockedSecretBytes {
    fn drop(&mut self) {
        // Zeroize first to ensure data is erased before unlocking
        use zeroize::Zeroize;
        self.bytes.zeroize();
        // Then unlock the memory region
        if let Some(region) = &self.locked_region {
            unlock_memory_region(region.base, region.len);
        }
    }
}

// Note: Clone is intentionally not implemented to prevent accidental
// duplication of sensitive data. This type cannot be cloned.

impl fmt::Debug for LockedSecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockedSecretBytes")
            .field("bytes", &"***REDACTED***")
            .finish()
    }
}

// =============================================================================
// LockedKey32
// =============================================================================

/// A 32-byte (256-bit) key stored in locked memory.
///
/// This type is used for storing cryptographic keys (e.g., Argon2 output,
/// wrapping keys, DEKs) in memory that cannot be swapped to disk.
///
/// # Examples
///
/// ```
/// use oak_keyring::security::LockedKey32;
///
/// // Create from existing key material
/// let key = LockedKey32::new([42u8; 32]).unwrap();
///
/// // Generate key material using a function
/// let key = LockedKey32::generate_from(|slice| {
///     slice.fill(123);
///     Ok(())
/// }).unwrap();
/// ```
pub struct LockedKey32 {
    bytes: LockedSecretBytes,
}

impl LockedKey32 {
    /// Creates a new locked key from existing key material.
    ///
    /// # Errors
    ///
    /// Returns an error if the memory lock fails.
    pub fn new(key: [u8; 32]) -> Result<Self, String> {
        let mut bytes = LockedSecretBytes::with_len(32)?;
        bytes.expose_mut().copy_from_slice(&key);
        Ok(Self { bytes })
    }

    /// Exposes the underlying 32-byte key.
    pub fn expose(&self) -> &[u8; 32] {
        // SAFETY: LockedKey32 always stores exactly 32 bytes
        self.bytes.expose().try_into().unwrap()
    }

    /// Generates a key using the provided function.
    ///
    /// The function receives a mutable 32-byte slice and should fill it
    /// with key material (e.g., from a KDF or RNG).
    ///
    /// # Errors
    ///
    /// Returns an error if the memory lock fails or if the generator fails.
    pub fn generate_from<F>(f: F) -> Result<Self, String>
    where
        F: FnOnce(&mut [u8]) -> Result<(), String>,
    {
        let mut bytes = LockedSecretBytes::with_len(32)?;
        f(bytes.expose_mut())?;
        Ok(Self { bytes })
    }
}

// Note: Clone is intentionally not implemented to prevent accidental
// duplication of sensitive key material. This type cannot be cloned.

impl fmt::Debug for LockedKey32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockedKey32")
            .field("bytes", &"***REDACTED***")
            .finish()
    }
}
