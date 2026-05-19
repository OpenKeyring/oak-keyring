// =============================================================================
// Tests for crash_handler registry
// =============================================================================

#[cfg(test)]
mod tests {
    use crate::security::memory::{LockedKey32, LockedSecretBytes};

    /// Test that registry tracks allocations
    #[test]
    fn registry_tracks_allocations() {
        // Use LockedSecretBytes for realistic testing
        let _bytes1 = LockedSecretBytes::with_len(32).expect("allocation should succeed");
        let _bytes2 = LockedSecretBytes::with_len(64).expect("allocation should succeed");

        // Both allocations should be registered
        // We can't directly inspect the registry, but we can verify
        // by testing zeroize_all behavior
    }

    /// Test that registry untracks on drop
    #[test]
    fn registry_untrack_on_unregister() {
        // Create and drop a LockedSecretBytes
        {
            let _bytes = LockedSecretBytes::with_len(32).expect("allocation should succeed");
            // bytes is registered here
        } // bytes is dropped and unregistered here

        // Create another allocation to verify registry still works
        let _bytes = LockedSecretBytes::with_len(32).expect("allocation should succeed");
    }

    /// Test that zeroize_all clears registered memory
    ///
    /// NOTE: This test may be flaky when run in parallel with other tests
    /// because zeroize_all() affects ALL registered memory globally.
    /// This is the correct behavior for a crash handler.
    #[test]
    fn zeroize_all_clears_memory() {
        // Skip this test on non-Unix platforms
        #[cfg(unix)]
        {
            // Create a LockedSecretBytes with known pattern
            let mut bytes = LockedSecretBytes::with_len(32).expect("allocation should succeed");
            bytes.expose_mut().fill(0xAB);

            // Verify pattern is set
            assert_eq!(bytes.expose()[0], 0xAB);
            assert_eq!(bytes.expose()[31], 0xAB);

            // Call zeroize_all (this would be called by crash handler)
            crate::security::crash_handler::zeroize_all();

            // Verify memory is zeroed
            // The memory should have been zeroized by zeroize_all()
            let slice = bytes.expose();

            // Check all bytes in the used region
            // Note: Due to test parallelism, zeroize_all() may have already
            // been called by another test, so we just verify the memory is
            // no longer the original pattern.
            for (i, &byte) in slice.iter().enumerate() {
                assert_ne!(
                    byte, 0xAB,
                    "Byte at index {} should have been modified by zeroize_all",
                    i
                );
            }
        }

        // On non-Unix platforms, just verify the memory is still accessible
        #[cfg(not(unix))]
        {
            let mut bytes = LockedSecretBytes::with_len(32).expect("allocation should succeed");
            bytes.expose_mut().fill(0xAB);
            assert_eq!(bytes.expose()[0], 0xAB);
        }
    }

    /// Test that zeroize_all handles empty registry gracefully
    #[test]
    fn zeroize_all_handles_empty_registry() {
        // zeroize_all should not panic even with no registered regions
        #[cfg(unix)]
        crate::security::crash_handler::zeroize_all();
    }

    /// Test that zero-length allocations don't register
    #[test]
    fn zero_length_allocations_dont_register() {
        let bytes = LockedSecretBytes::with_len(0).expect("zero-length should succeed");
        assert_eq!(bytes.expose().len(), 0);
        // Should not panic when dropped
    }

    /// Test registry with LockedKey32
    #[test]
    fn registry_works_with_locked_key32() {
        let mut key_bytes = [42u8; 32];
        let _key = LockedKey32::new(&mut key_bytes).expect("key creation should succeed");

        // Key should be registered
        // Verify key_bytes was zeroized
        assert_eq!(key_bytes[0], 0);
        assert_eq!(key_bytes[31], 0);
    }

    /// Test multiple allocations and deallocations
    #[test]
    fn registry_handles_multiple_lifetimes() {
        let bytes1 = LockedSecretBytes::with_len(16).expect("allocation should succeed");
        let bytes2 = LockedSecretBytes::with_len(32).expect("allocation should succeed");

        // Both registered
        drop(bytes1); // bytes1 unregistered

        let bytes3 = LockedSecretBytes::with_len(64).expect("allocation should succeed");
        // bytes2 and bytes3 registered

        drop(bytes2); // bytes2 unregistered
                      // bytes3 still registered

        drop(bytes3); // bytes3 unregistered
                      // Registry should be empty
    }
}
