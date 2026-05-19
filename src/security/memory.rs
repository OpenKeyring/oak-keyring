// =============================================================================
// Locked Memory Primitives
// =============================================================================
//
// This module provides memory locking primitives to prevent sensitive data
// (encryption keys, passwords) from being swapped to disk.
//
// Platform-specific implementation:
// - Unix (macOS, Linux): uses mmap() for page-exclusive allocation,
//   mlock() to prevent swapping, and MADV_DONTDUMP on Linux to exclude
//   from core dumps.
// - Windows: uses VirtualAlloc() for page-exclusive allocation and
//   VirtualLock() to prevent swapping.
//
// Each allocated region occupies an exclusive set of pages so that
// mlock/munlock on one region does not affect other regions.
//
// Safety considerations:
// - LockedSecretBytes stores a raw pointer - NOT Clone/Copy
// - Drop must zeroize before unlocking and freeing pages
// - LockedKey32 is NOT Clone (prevents key duplication)
// - Debug shows "***REDACTED***" to prevent leaks through logs

use std::fmt;

// =============================================================================
// Platform-specific page-exclusive allocation
// =============================================================================

/// Allocates one or more locked, page-exclusive memory regions.
///
/// Returns a tuple of (pointer, total_allocated_size) where the pointer is
/// page-aligned and the size is a multiple of the system page size.
#[cfg(unix)]
fn allocate_locked_pages(min_len: usize) -> Result<(*mut u8, usize), String> {
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
    let aligned_len = (min_len + page_size - 1) & !(page_size - 1);

    // SAFETY: mmap allocates a new page-aligned region that is exclusive to
    // this allocation. MAP_PRIVATE|MAP_ANONYMOUS creates a zero-initialized
    // private mapping not backed by any file.
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            aligned_len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };

    if ptr == libc::MAP_FAILED {
        return Err(format!("mmap failed: {}", std::io::Error::last_os_error()));
    }

    let ptr = ptr as *mut u8;

    // Lock the pages to prevent swapping to disk.
    let result = unsafe { libc::mlock(ptr as *const libc::c_void, aligned_len) };
    if result != 0 {
        let err = std::io::Error::last_os_error();
        unsafe { libc::munmap(ptr as *mut libc::c_void, aligned_len) };
        return Err(format!("mlock failed: {err}"));
    }

    #[cfg(target_os = "linux")]
    {
        // Exclude from core dumps so keys don't leak via crash reports.
        let result =
            unsafe { libc::madvise(ptr as *mut libc::c_void, aligned_len, libc::MADV_DONTDUMP) };
        if result != 0 {
            let err = std::io::Error::last_os_error();
            unsafe {
                libc::munlock(ptr as *const libc::c_void, aligned_len);
                libc::munmap(ptr as *mut libc::c_void, aligned_len);
            }
            return Err(format!("madvise(MADV_DONTDUMP) failed: {err}"));
        }
    }

    Ok((ptr, aligned_len))
}

/// Frees a locked, page-exclusive memory region previously allocated by
/// [`allocate_locked_pages`].
#[cfg(unix)]
fn free_locked_pages(ptr: *mut u8, len: usize) {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::madvise(ptr as *mut libc::c_void, len, libc::MADV_DODUMP);
    }
    unsafe {
        libc::munlock(ptr as *const libc::c_void, len);
        libc::munmap(ptr as *mut libc::c_void, len);
    }
}

#[cfg(windows)]
fn allocate_locked_pages(min_len: usize) -> Result<(*mut u8, usize), String> {
    use windows_sys::Win32::System::Memory::{
        VirtualAlloc, VirtualLock, MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE,
    };

    let page_size = unsafe {
        let mut info = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetSystemInfo(&mut info);
        info.dwPageSize as usize
    };

    let aligned_len = (min_len + page_size - 1) & !(page_size - 1);

    // SAFETY: VirtualAlloc reserves and commits page-aligned memory that is
    // exclusive to this allocation.
    let ptr = unsafe {
        VirtualAlloc(
            std::ptr::null(),
            aligned_len,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };

    if ptr.is_null() {
        return Err(format!(
            "VirtualAlloc failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    let result = unsafe { VirtualLock(ptr, aligned_len) };
    if result == 0 {
        let err = std::io::Error::last_os_error();
        unsafe {
            VirtualFree(ptr, 0, MEM_RELEASE);
        }
        return Err(format!("VirtualLock failed: {err}"));
    }

    Ok((ptr as *mut u8, aligned_len))
}

#[cfg(windows)]
fn free_locked_pages(ptr: *mut u8, len: usize) {
    use windows_sys::Win32::System::Memory::{VirtualFree, VirtualUnlock, MEM_RELEASE};

    unsafe {
        VirtualUnlock(ptr as *const _, len);
        VirtualFree(ptr as *mut _, 0, MEM_RELEASE);
    }
}

// =============================================================================
// LockedSecretBytes
// =============================================================================

/// A byte buffer stored in locked, page-exclusive memory.
///
/// Unlike a Vec-backed approach where multiple buffers could share a page
/// (and one buffer's munlock would unprotect others), this type uses
/// platform-specific page-exclusive allocation (mmap on Unix, VirtualAlloc
/// on Windows) so that each instance occupies its own set of pages.
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
/// let bytes = LockedSecretBytes::with_len(32).expect("memory lock should succeed");
/// // ... use the bytes ...
/// // When dropped, the memory is zeroized and unlocked
/// ```
pub struct LockedSecretBytes {
    /// Pointer to the start of the mmap'd / VirtualAlloc'd region.
    /// For zero-length instances this is null.
    ptr: *mut u8,

    /// Number of usable bytes (the originally requested length). Must be <= cap.
    len: usize,

    /// Total allocated bytes (page-aligned). 0 for zero-length instances.
    cap: usize,
}

impl LockedSecretBytes {
    /// Creates a new byte buffer with the specified length.
    ///
    /// Memory is allocated in exclusive page(s) using mmap (Unix) or
    /// VirtualAlloc (Windows). Memory locking must succeed for non-empty
    /// buffers.
    ///
    /// A length of 0 creates an empty buffer without allocating or locking.
    pub fn with_len(len: usize) -> Result<Self, String> {
        if len == 0 {
            return Ok(Self {
                ptr: std::ptr::null_mut(),
                len: 0,
                cap: 0,
            });
        }

        let (ptr, cap) = allocate_locked_pages(len)?;

        // Register for crash-time zeroization
        #[cfg(unix)]
        crate::security::crash_handler::register(ptr, cap);

        Ok(Self { ptr, len, cap })
    }

    /// Exposes the underlying bytes as a slice.
    pub fn expose(&self) -> &[u8] {
        if self.len == 0 {
            return &[];
        }
        // SAFETY: ptr points to valid, initialized memory of at least len
        // bytes (enforced by with_len). The memory is locked and exclusive.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// Exposes the underlying bytes as a mutable slice.
    pub fn expose_mut(&mut self) -> &mut [u8] {
        if self.len == 0 {
            return &mut [];
        }
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }

    /// Sets memory protection for the underlying page(s).
    ///
    /// On Unix, this calls `mprotect` with the given protection flags.
    /// On non-Unix, this is a no-op.
    #[cfg(unix)]
    fn set_prot(&self, prot: libc::c_int) {
        if self.cap == 0 {
            return;
        }
        // SAFETY: ptr is page-aligned (from mmap), cap is a multiple of
        // page size. mprotect only changes page protection bits.
        let result = unsafe { libc::mprotect(self.ptr as *mut libc::c_void, self.cap, prot) };
        if result != 0 {
            tracing::warn!("mprotect failed: {}", std::io::Error::last_os_error());
        }
    }

    /// Sets memory protection (no-op on non-Unix).
    #[cfg(not(unix))]
    fn set_prot(&self, _prot: i32) {}
}

impl Drop for LockedSecretBytes {
    fn drop(&mut self) {
        if self.cap == 0 {
            return;
        }

        // Unregister from crash handler registry
        #[cfg(unix)]
        crate::security::crash_handler::unregister(self.ptr);

        // Zeroize the used portion before unlocking and freeing pages.
        use zeroize::Zeroize;
        self.expose_mut().zeroize();

        // Set PROT_NONE so XNU skips these pages in any core dump
        // generated during or after Drop. Pages are already zeroized.
        #[cfg(unix)]
        self.set_prot(libc::PROT_NONE);

        // Free the allocated locked pages (this also unlocks them).
        free_locked_pages(self.ptr, self.cap);
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
/// let mut key_bytes = [42u8; 32];
/// let key = LockedKey32::new(&mut key_bytes).expect("memory lock should succeed");
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
    /// Creates a new key from existing key material.
    ///
    /// The key is copied into locked memory and the source array
    /// is zeroized after copying through the mutable reference.
    pub fn new(key: &mut [u8; 32]) -> Result<Self, String> {
        let mut bytes = LockedSecretBytes::with_len(32)?;
        bytes.expose_mut().copy_from_slice(&*key);
        use zeroize::Zeroize;
        key.zeroize();
        Ok(Self { bytes })
    }

    /// Exposes the underlying 32-byte key.
    pub fn expose(&self) -> &[u8; 32] {
        // SAFETY: LockedKey32 always stores exactly 32 bytes
        self.bytes.expose().try_into().unwrap()
    }

    /// Exposes the key for the duration of the closure with automatic
    /// memory protection. On Unix, the page is set to PROT_READ during
    /// the closure and PROT_NONE after.
    pub fn with_exposed<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> R {
        #[cfg(unix)]
        self.bytes.set_prot(libc::PROT_READ);

        let result = f(self.expose());

        #[cfg(unix)]
        self.bytes.set_prot(libc::PROT_NONE);

        result
    }

    /// Exposes the key mutably for the duration of the closure with
    /// automatic memory protection.
    pub fn with_exposed_mut<R>(&mut self, f: impl FnOnce(&mut [u8; 32]) -> R) -> R {
        #[cfg(unix)]
        self.bytes.set_prot(libc::PROT_READ | libc::PROT_WRITE);

        // SAFETY: LockedKey32 always stores exactly 32 bytes
        let key: &mut [u8; 32] = self.bytes.expose_mut().try_into().unwrap();
        let result = f(key);

        #[cfg(unix)]
        self.bytes.set_prot(libc::PROT_NONE);

        result
    }

    /// Generates a key using the provided function.
    ///
    /// The function receives a mutable 32-byte slice and should fill it
    /// with key material (e.g., from a KDF or RNG).
    ///
    /// # Errors
    ///
    /// Returns an error if the generator function fails.
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
