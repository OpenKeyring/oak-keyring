use crate::security::{LockedKey32, LockedSecretBytes};

#[test]
fn locked_key32_exposes_32_bytes() {
    let mut key_data = [7u8; 32];
    let key = LockedKey32::new(&mut key_data).expect("lock should succeed in normal test env");
    assert_eq!(key.expose(), &[7u8; 32]);
}

#[test]
fn locked_secret_bytes_zero_len_is_allowed() {
    let bytes = LockedSecretBytes::with_len(0).expect("zero length should not lock");
    assert!(bytes.expose().is_empty());
}

#[test]
fn locked_secret_bytes_with_len_creates_correct_size() {
    let bytes = LockedSecretBytes::with_len(16).expect("lock should succeed in normal test env");
    assert_eq!(bytes.expose().len(), 16);
}

#[test]
fn locked_key32_generate_from_works() {
    let key = LockedKey32::generate_from(|slice: &mut [u8]| {
        slice.fill(42);
        Ok(())
    })
    .expect("generation should succeed");
    assert_eq!(key.expose(), &[42u8; 32]);
}

#[test]
fn locked_secret_bytes_expose_mut_allows_modification() {
    let mut bytes = LockedSecretBytes::with_len(8).expect("lock should succeed in normal test env");
    bytes
        .expose_mut()
        .copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(bytes.expose(), &[1, 2, 3, 4, 5, 6, 7, 8]);
}
