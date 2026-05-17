use crate::security::LockedKey32;

/// A database page key stored in locked memory.
///
/// This type wraps [`LockedKey32`] to prevent the database page key from being
/// swapped to disk or appearing in core dumps. Used specifically for SQLCipher
/// database encryption.
///
/// This type is intentionally `!Clone` as the key material must not be
/// duplicated.
pub struct DbPageKey(LockedKey32);

// SAFETY: LockedKey32 uses mlock/VirtualLock which are thread-safe on all
// platforms. The wrapped key material cannot be cloned (LockedKey32 is
// intentionally !Clone). This follows the same pattern as SecretKey/DEK.
unsafe impl Send for DbPageKey {}
unsafe impl Sync for DbPageKey {}

impl DbPageKey {
    /// Creates a new database page key from existing key material.
    ///
    /// The key material is copied into locked memory and the source array
    /// is zeroized after copying through the mutable reference.
    pub fn new(bytes: &mut [u8; 32]) -> Result<Self, String> {
        Ok(Self(LockedKey32::new(bytes)?))
    }

    /// Exposes the underlying 32-byte key.
    pub fn expose(&self) -> &[u8; 32] {
        self.0.expose()
    }
}

impl std::fmt::Debug for DbPageKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("DbPageKey").field(&"<locked>").finish()
    }
}

#[cfg(any(test, feature = "sqlcipher"))]
pub fn test_db_page_key(bytes: [u8; 32]) -> DbPageKey {
    let mut bytes = bytes;
    DbPageKey::new(&mut bytes).expect("create test DbPageKey")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_page_key_wraps_32_bytes() {
        let mut bytes = [0x42u8; 32];
        let key = DbPageKey::new(&mut bytes).expect("create DbPageKey");
        assert_eq!(key.expose().len(), 32);
        assert_eq!(bytes, [0u8; 32], "source bytes must be zeroized");
    }

    #[test]
    fn db_page_key_debug_does_not_expose_material() {
        let mut bytes = [0xabu8; 32];
        let key = DbPageKey::new(&mut bytes).expect("create DbPageKey");
        let rendered = format!("{key:?}");
        assert!(rendered.contains("DbPageKey"));
        assert!(!rendered.contains("abab"));
    }
}
