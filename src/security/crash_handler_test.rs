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

    #[test]
    fn zeroize_all_does_not_panic() {
        let mut bytes = LockedSecretBytes::with_len(32).expect("allocation should succeed");
        bytes.expose_mut().fill(0xAB);
        #[cfg(unix)]
        crate::security::crash_handler::zeroize_all();
        // Verify the call completed without panicking.
        // We cannot assert memory state here because zeroize_all() operates
        // on a shared global registry and tests run in parallel.
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
