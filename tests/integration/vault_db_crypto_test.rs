use oak_keyring::crypto::bip39::Passkey;
use oak_keyring::crypto::payload::{
    decrypt_name_only, decrypt_payload, decrypt_subtitle, encrypt_payload,
};
use oak_keyring::crypto::CryptoManager;
use oak_keyring::crypto::MnemonicLanguage;
use oak_keyring::types::credential::{CredentialType, EncryptedPayload};
use oak_keyring::types::sensitive::SecureStr;

/// Helper: create an unlocked CryptoManager from a fresh mnemonic.
fn unlocked_crypto_manager() -> (Passkey, CryptoManager) {
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
    let mut cm = CryptoManager::new();
    cm.unlock_with_mnemonic(&mnemonic).unwrap();
    (mnemonic, cm)
}

// ─── CryptoManager Lifecycle Tests ──────────────────────────────────

#[test]
fn test_crypto_manager_lock_clears_keystore() {
    let (_, mut cm) = unlocked_crypto_manager();
    assert!(
        cm.is_unlocked(),
        "should be unlocked after unlock_with_mnemonic"
    );

    cm.lock();
    assert!(
        !cm.is_unlocked(),
        "is_unlocked must return false after lock()"
    );
}

#[test]
fn test_unlock_with_mnemonic_flow() {
    // Verifies the full Passkey → SK → KEK → DEK derivation chain.
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
    let mut cm = CryptoManager::new();
    assert!(!cm.is_unlocked(), "new CryptoManager should start locked");

    cm.unlock_with_mnemonic(&mnemonic).unwrap();
    assert!(
        cm.is_unlocked(),
        "should be unlocked after unlock_with_mnemonic"
    );

    // Verify DEK derivation works (the get_dek internally calls HKDF).
    let dek = cm.get_dek(1).unwrap();
    assert_eq!(dek.as_bytes().len(), 32, "DEK must be 32 bytes");
}

#[test]
fn test_encrypt_decrypt_roundtrip() {
    let (_, cm) = unlocked_crypto_manager();
    let plaintext = b"hello world secret data";
    let aad = b"record-uuid-123";

    let (ciphertext, nonce) = cm.encrypt(plaintext, aad).unwrap();
    assert_ne!(
        ciphertext.as_slice(),
        plaintext,
        "ciphertext must differ from plaintext"
    );

    let decrypted = cm
        .decrypt(&ciphertext, &nonce, aad, cm.current_dek_version())
        .unwrap();
    assert_eq!(decrypted, plaintext, "decrypted text must match original");
}

#[test]
fn test_decrypt_wrong_aad_fails() {
    let (_, cm) = unlocked_crypto_manager();
    let plaintext = b"sensitive data";
    let aad = b"correct-aad";
    let wrong_aad = b"wrong-aad";

    let (ciphertext, nonce) = cm.encrypt(plaintext, aad).unwrap();
    let result = cm.decrypt(&ciphertext, &nonce, wrong_aad, cm.current_dek_version());

    assert!(result.is_err(), "decrypt with wrong AAD must fail");
}

#[test]
fn test_different_nonces_per_encrypt() {
    let (_, cm) = unlocked_crypto_manager();
    let plaintext = b"same plaintext each time";
    let aad = b"aad";

    let (ct1, nonce1) = cm.encrypt(plaintext, aad).unwrap();
    let (ct2, nonce2) = cm.encrypt(plaintext, aad).unwrap();

    assert_ne!(
        nonce1, nonce2,
        "each encryption must produce a different nonce (randomness)"
    );
    // Ciphertexts should also differ due to different nonces.
    assert_ne!(
        ct1, ct2,
        "ciphertexts must differ for same plaintext with different nonces"
    );
}

// ─── Payload Integration Tests ──────────────────────────────────────

/// Helper: create a Login-type EncryptedPayload for testing.
fn sample_login_payload() -> EncryptedPayload {
    EncryptedPayload::Login {
        name: "GitHub".to_string(),
        username: "alice".to_string(),
        password: SecureStr::new("s3cret!".to_string()),
        url: Some("https://github.com".to_string()),
        notes: None,
    }
}

/// Helper: create an Api-type EncryptedPayload for testing.
fn sample_api_payload() -> EncryptedPayload {
    EncryptedPayload::Api {
        name: "Stripe".to_string(),
        app_id: "pk_live_abc123".to_string(),
        secret_key: SecureStr::new("sk_live_xyz789".to_string()),
        url: None,
        notes: None,
    }
}

#[test]
fn test_encrypt_payload_roundtrip() {
    let (_, cm) = unlocked_crypto_manager();
    let payload = sample_login_payload();
    let aad = b"record-uuid-login-001";

    let (ciphertext, nonce) = encrypt_payload(&cm, &payload, aad).unwrap();

    let decrypted = decrypt_payload(
        &cm,
        &ciphertext,
        &nonce,
        aad,
        CredentialType::Login,
        cm.current_dek_version(),
    )
    .unwrap();

    // Verify name matches (common field).
    assert_eq!(decrypted.name(), "GitHub");

    // Verify the credential type is preserved.
    assert_eq!(decrypted.credential_type(), CredentialType::Login);
}

#[test]
fn test_decrypt_name_only_matches_full() {
    let (_, cm) = unlocked_crypto_manager();
    let payload = sample_login_payload();
    let aad = b"record-uuid-name-test";

    let (ciphertext, nonce) = encrypt_payload(&cm, &payload, aad).unwrap();

    let name_only =
        decrypt_name_only(&cm, &ciphertext, &nonce, aad, cm.current_dek_version()).unwrap();
    let full = decrypt_payload(
        &cm,
        &ciphertext,
        &nonce,
        aad,
        CredentialType::Login,
        cm.current_dek_version(),
    )
    .unwrap();

    assert_eq!(
        name_only,
        full.name(),
        "decrypt_name_only must match name from full decrypt"
    );
    assert_eq!(name_only, "GitHub");
}

#[test]
fn test_decrypt_subtitle_login() {
    let (_, cm) = unlocked_crypto_manager();
    let payload = sample_login_payload();
    let aad = b"record-uuid-subtitle-login";

    let (ciphertext, nonce) = encrypt_payload(&cm, &payload, aad).unwrap();

    let subtitle = decrypt_subtitle(
        &cm,
        &ciphertext,
        &nonce,
        aad,
        CredentialType::Login,
        cm.current_dek_version(),
    )
    .unwrap();
    assert_eq!(
        subtitle, "alice",
        "Login subtitle must return the username field"
    );
}

#[test]
fn test_decrypt_subtitle_api() {
    let (_, cm) = unlocked_crypto_manager();
    let payload = sample_api_payload();
    let aad = b"record-uuid-subtitle-api";

    let (ciphertext, nonce) = encrypt_payload(&cm, &payload, aad).unwrap();

    let subtitle = decrypt_subtitle(
        &cm,
        &ciphertext,
        &nonce,
        aad,
        CredentialType::Api,
        cm.current_dek_version(),
    )
    .unwrap();
    assert_eq!(
        subtitle, "pk_live_abc123",
        "Api subtitle must return the app_id field"
    );
}

// ─── Cross-Device Determinism Test ──────────────────────────────────

#[test]
fn test_cross_device_determinism() {
    // Same mnemonic → same SK → same KEK → same DEK.
    // Simulates two devices restoring from the same recovery phrase.
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();

    let mut cm1 = CryptoManager::new();
    cm1.unlock_with_mnemonic(&mnemonic).unwrap();

    let mut cm2 = CryptoManager::new();
    cm2.unlock_with_mnemonic(&mnemonic).unwrap();

    let dek1 = cm1.get_dek(1).unwrap();
    let dek2 = cm2.get_dek(1).unwrap();

    assert_eq!(
        dek1.as_bytes(),
        dek2.as_bytes(),
        "same mnemonic must derive identical DEK across CryptoManager instances"
    );

    // Cross-device encrypt/decrypt: encrypt on device 1, decrypt on device 2.
    let plaintext = b"cross-device secret";
    let aad = b"sync-record-id";

    let (ciphertext, nonce) = cm1.encrypt(plaintext, aad).unwrap();
    let decrypted = cm2
        .decrypt(&ciphertext, &nonce, aad, cm1.current_dek_version())
        .unwrap();
    assert_eq!(
        decrypted, plaintext,
        "data encrypted on device 1 must decrypt on device 2"
    );
}
