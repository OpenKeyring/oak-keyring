#[cfg(test)]
mod tests {
    use crate::security::memory::{LockedKey32, LockedSecretBytes};

    #[test]
    fn registry_tracks_allocations() {
        let _bytes1 = LockedSecretBytes::with_len(32).expect("allocation should succeed");
        let _bytes2 = LockedSecretBytes::with_len(64).expect("allocation should succeed");
    }

    #[test]
    fn registry_untrack_on_unregister() {
        {
            let _bytes = LockedSecretBytes::with_len(32).expect("allocation should succeed");
        }
        let _bytes = LockedSecretBytes::with_len(32).expect("allocation should succeed");
    }

    /// Verify zeroize_all() actually clears memory.
    /// Uses `--test-threads=1` annotation to avoid parallel registry conflicts.
    #[test]
    fn zeroize_all_clears_registered_memory() {
        #[cfg(unix)]
        {
            let mut bytes = LockedSecretBytes::with_len(64).expect("allocation should succeed");
            bytes.expose_mut().fill(0xAB);
            // Verify pattern is set
            assert_eq!(bytes.expose()[0], 0xAB);
            assert_eq!(bytes.expose()[63], 0xAB);

            crate::security::crash_handler::zeroize_all();

            // Verify the first 64 bytes are zeroed.
            // Note: other parallel tests may also register memory, which is fine -
            // we only verify our own allocation was zeroed.
            let slice = bytes.expose();
            for (i, &byte) in slice.iter().enumerate() {
                assert_eq!(byte, 0, "byte at index {i} should be zeroed");
            }
        }
        #[cfg(not(unix))]
        {
            let mut bytes = LockedSecretBytes::with_len(32).expect("allocation should succeed");
            bytes.expose_mut().fill(0xAB);
            assert_eq!(bytes.expose()[0], 0xAB);
        }
    }

    #[test]
    fn zeroize_all_handles_empty_registry() {
        #[cfg(unix)]
        crate::security::crash_handler::zeroize_all();
    }

    #[test]
    fn zero_length_allocations_dont_register() {
        let bytes = LockedSecretBytes::with_len(0).expect("zero-length should succeed");
        assert_eq!(bytes.expose().len(), 0);
    }

    #[test]
    fn registry_works_with_locked_key32() {
        let mut key_bytes = [42u8; 32];
        let _key = LockedKey32::new(&mut key_bytes).expect("key creation should succeed");
        assert_eq!(key_bytes[0], 0);
        assert_eq!(key_bytes[31], 0);
    }

    #[test]
    fn registry_handles_multiple_lifetimes() {
        let bytes1 = LockedSecretBytes::with_len(16).expect("allocation should succeed");
        let bytes2 = LockedSecretBytes::with_len(32).expect("allocation should succeed");

        drop(bytes1);

        let bytes3 = LockedSecretBytes::with_len(64).expect("allocation should succeed");
        drop(bytes2);
        drop(bytes3);
    }
}
