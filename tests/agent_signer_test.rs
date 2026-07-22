//! Integration tests for the SSH agent signer layer (`oak_keyring::agent::signer`).
//!
//! These tests exercise `Ed25519Signer` end-to-end: an OpenSSH PEM is parsed,
//! a message is signed, and the returned wire-format signature blob is parsed
//! and verified directly with `ed25519-dalek` (NOT via `ssh_key::SshSig`,
//! because an SSH agent `SIGN_RESPONSE` carries a raw algorithm-specific
//! signature blob, not an SSHSIG wrapper).

use oak_keyring::agent::signer::{Ed25519Signer, SignFlags, SshAlgo, SshSigner};

/// Parse an SSH agent ed25519 wire-format signature blob and return the raw
/// 64-byte signature.
///
/// Wire layout: `string "ssh-ed25519"` + `string <64-byte sig>`, where every
/// `string` is a 4-byte big-endian length prefix followed by the bytes.
fn extract_ed25519_sig(blob: &[u8]) -> &[u8] {
    use std::convert::TryInto;
    assert!(
        blob.len() >= 4,
        "blob too short for algorithm-name length prefix"
    );
    let (alg_len_bytes, rest) = blob.split_at(4);
    let alg_len = u32::from_be_bytes(alg_len_bytes.try_into().unwrap()) as usize;
    assert_eq!(
        alg_len,
        b"ssh-ed25519".len(),
        "algorithm name length must be 'ssh-ed25519' (11)"
    );
    let (alg, rest) = rest.split_at(alg_len);
    assert_eq!(alg, b"ssh-ed25519", "algorithm name must be 'ssh-ed25519'");

    let (sig_len_bytes, rest) = rest.split_at(4);
    let sig_len = u32::from_be_bytes(sig_len_bytes.try_into().unwrap()) as usize;
    assert_eq!(sig_len, 64, "ed25519 signature must be 64 bytes");
    let (sig, tail) = rest.split_at(sig_len);
    assert!(tail.is_empty(), "trailing bytes after ed25519 signature");
    sig
}

/// Extract the 32-byte ed25519 public key from an OpenSSH PEM via ssh-key,
/// for independent verification on the public-key side.
fn ed25519_public_bytes(pem: &str) -> [u8; 32] {
    let private = ssh_key::PrivateKey::from_openssh(pem).expect("PEM must parse");
    match private.public_key().key_data() {
        ssh_key::public::KeyData::Ed25519(pk) => *pk.as_ref(),
        other => panic!("expected ed25519 key, got {other:?}"),
    }
}

#[test]
fn ed25519_sign_is_verifiable() {
    let pem = include_str!("fixtures/test_ed25519");
    let signer = Ed25519Signer::from_openssh(pem, None).expect("unencrypted key must load");

    assert_eq!(signer.algorithm(), SshAlgo::Ed25519);

    let data = b"authenticate me";
    let blob = signer
        .sign(data, SignFlags::default())
        .expect("signing must succeed");

    // Parse the wire blob and verify the 64-byte sig with ed25519-dalek.
    let sig_bytes = extract_ed25519_sig(&blob);
    let sig = ed25519_dalek::Signature::from_bytes(sig_bytes.try_into().unwrap());
    let pub_bytes = ed25519_public_bytes(pem);
    let vk = ed25519_dalek::VerifyingKey::from_bytes(&pub_bytes).expect("valid public key");
    use ed25519_dalek::Verifier;
    vk.verify(data, &sig)
        .expect("signature must verify against the public key");
}

#[test]
fn ed25519_sign_differs_per_message_and_is_deterministic() {
    let pem = include_str!("fixtures/test_ed25519");
    let signer = Ed25519Signer::from_openssh(pem, None).expect("unencrypted key must load");

    // ed25519 is deterministic: signing the same data twice yields the same sig.
    let blob_a = signer.sign(b"message-a", SignFlags::default()).unwrap();
    let blob_a2 = signer.sign(b"message-a", SignFlags::default()).unwrap();
    assert_eq!(blob_a, blob_a2, "ed25519 signing must be deterministic");

    // Different data yields a different signature blob.
    let blob_b = signer.sign(b"message-b", SignFlags::default()).unwrap();
    assert_ne!(blob_a, blob_b, "different messages must differ");

    // The blob for message-b must also verify.
    let sig_bytes = extract_ed25519_sig(&blob_b);
    let sig = ed25519_dalek::Signature::from_bytes(sig_bytes.try_into().unwrap());
    let pub_bytes = ed25519_public_bytes(pem);
    let vk = ed25519_dalek::VerifyingKey::from_bytes(&pub_bytes).unwrap();
    use ed25519_dalek::Verifier;
    vk.verify(b"message-b", &sig).expect("must verify");
}

#[test]
fn ed25519_sign_flags_are_ignored_for_ed25519() {
    // RSA-only flags must be accepted (no-op) by the ed25519 signer.
    let pem = include_str!("fixtures/test_ed25519");
    let signer = Ed25519Signer::from_openssh(pem, None).expect("unencrypted key must load");
    let flags = SignFlags {
        rsa_sha2_256: true,
        rsa_sha2_512: true,
    };
    let blob = signer.sign(b"data", flags).expect("flags must not error");
    assert!(!blob.is_empty());
}

#[test]
fn ed25519_public_key_ssh_is_openssh_format() {
    let pem = include_str!("fixtures/test_ed25519");
    let signer = Ed25519Signer::from_openssh(pem, None).expect("unencrypted key must load");
    let public_ssh = signer.public_key_ssh().expect("public key string");
    assert!(
        public_ssh.starts_with("ssh-ed25519 "),
        "public key must be OpenSSH format, got: {public_ssh}"
    );
}

#[test]
fn ed25519_passphrase_protected_key_loads_and_signs() {
    let pem = include_str!("fixtures/test_ed25519_encrypted");
    let signer = Ed25519Signer::from_openssh(pem, Some("test-passphrase-123"))
        .expect("passphrase-protected key must decrypt");

    let data = b"encrypted-key-sign-test";
    let blob = signer
        .sign(data, SignFlags::default())
        .expect("signing must succeed");

    let sig_bytes = extract_ed25519_sig(&blob);
    let sig = ed25519_dalek::Signature::from_bytes(sig_bytes.try_into().unwrap());
    let pub_bytes = ed25519_public_bytes(pem);
    let vk = ed25519_dalek::VerifyingKey::from_bytes(&pub_bytes).unwrap();
    use ed25519_dalek::Verifier;
    vk.verify(data, &sig).expect("signature must verify");
}

#[test]
fn ed25519_passphrase_protected_key_wrong_passphrase_fails() {
    let pem = include_str!("fixtures/test_ed25519_encrypted");
    let result = Ed25519Signer::from_openssh(pem, Some("wrong-passphrase"));
    assert!(
        result.is_err(),
        "wrong passphrase must fail, not silently load or panic"
    );
}

#[test]
fn ed25519_passphrase_supplied_for_unencrypted_key_is_rejected() {
    // A passphrase supplied for an unencrypted key is a caller error; reject it
    // loudly rather than ignoring it (fail loud per project rules).
    let pem = include_str!("fixtures/test_ed25519");
    let result = Ed25519Signer::from_openssh(pem, Some("unused-passphrase"));
    assert!(
        result.is_err(),
        "passphrase on an unencrypted key must be rejected, not ignored"
    );
}

#[test]
fn ed25519_missing_passphrase_for_encrypted_key_fails() {
    let pem = include_str!("fixtures/test_ed25519_encrypted");
    let result = Ed25519Signer::from_openssh(pem, None);
    assert!(
        result.is_err(),
        "encrypted key without passphrase must fail loudly"
    );
}
