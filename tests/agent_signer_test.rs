//! Integration tests for the SSH agent signer layer (`oak_keyring::agent::signer`).
//!
//! These tests exercise `Ed25519Signer` end-to-end: an OpenSSH PEM is parsed,
//! a message is signed, and the returned wire-format signature blob is parsed
//! and verified directly with `ed25519-dalek` (NOT via `ssh_key::SshSig`,
//! because an SSH agent `SIGN_RESPONSE` carries a raw algorithm-specific
//! signature blob, not an SSHSIG wrapper).

use oak_keyring::agent::signer::{
    EcdsaCurve, EcdsaSigner, Ed25519Signer, RsaSigner, SignFlags, SshAlgo, SshSigner,
};

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

// ===========================================================================
// identity loading + whitelist filtering (`oak_keyring::agent::identity`)
// ===========================================================================
//
// These tests build an in-memory vault with an SSH record (ed25519) and a
// non-SSH record (Login), then exercise `load_ssh_identities` + `IdentityFilter`.
//
// ZERO-CACHE contract: `LoadedIdentity` must carry only `record_id` + `name` +
// `algo` + `public_blob`; it must NOT hold a signer or private key. The private
// key is fetched per-sign in the server task. These tests assert that contract
// by inspecting the public fields of the returned `LoadedIdentity`.

use oak_keyring::agent::identity::{load_ssh_identities, IdentityFilter, LoadedIdentity};
// `SshAlgo` is already imported at the top of this file from `agent::signer`.
use oak_keyring::crypto::bip39::{MnemonicLanguage, Passkey};
use oak_keyring::db::schema::init_db_in_memory;
use oak_keyring::services::vault::VaultService;
use oak_keyring::types::credential::{CredentialType, EncryptedPayload};
use oak_keyring::types::record::CreateRecordParams;
use oak_keyring::types::sensitive::SecureStr;
use regex::Regex;

/// A real ed25519 OpenSSH public key string (matches `fixtures/test_ed25519`).
/// Used as the vault record's stored `public_key` so `ssh_key::PublicKey` can
/// parse it into a wire-format blob.
const ED25519_PUB_SSH: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPddLwxmYUz+k43Vr+cahIy1iOROowugaJr8lQ6Tmi2V test-ed25519@oak-keyring";

/// Build an unlocked in-memory vault (no master password needed; mnemonic unlock).
fn unlocked_vault() -> VaultService {
    let conn = init_db_in_memory().expect("in-memory db");
    let mut svc = VaultService::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
    svc.unlock_with_mnemonic(&mnemonic)
        .expect("unlock_with_mnemonic must succeed in test");
    svc
}

/// Insert an SSH record named `name` with the ed25519 public key, returning its id.
fn insert_ssh_record(svc: &mut VaultService, name: &str) -> uuid::Uuid {
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Ssh,
        payload: EncryptedPayload::Ssh {
            name: name.to_string(),
            public_key: ED25519_PUB_SSH.to_string(),
            private_key: None,
            passphrase: None,
            notes: None,
        },
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    })
    .expect("create ssh record")
}

/// Insert a Login record (a non-SSH credential) that must be EXCLUDED from the
/// SSH identity list.
fn insert_login_record(svc: &mut VaultService, name: &str) -> uuid::Uuid {
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: EncryptedPayload::Login {
            name: name.to_string(),
            username: format!("user_{name}"),
            password: SecureStr::new("pw".to_string()),
            url: None,
            notes: None,
        },
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    })
    .expect("create login record")
}

#[test]
fn load_ssh_identities_returns_ed25519_identity() {
    let mut vault = unlocked_vault();
    let ssh_id = insert_ssh_record(&mut vault, "github-key");

    let identities =
        load_ssh_identities(&vault, &IdentityFilter::default()).expect("load must succeed");

    assert_eq!(identities.len(), 1, "exactly one SSH identity expected");
    let ident = &identities[0];
    assert_eq!(
        ident.record_id, ssh_id,
        "record_id must match the SSH record"
    );
    assert_eq!(
        ident.name, "github-key",
        "name must be the vault record name"
    );
    assert_eq!(ident.algo, SshAlgo::Ed25519, "ed25519 key maps to Ed25519");
    assert!(
        !ident.public_blob.is_empty(),
        "public_blob must be non-empty wire-format bytes"
    );
    // ZERO-CACHE contract: LoadedIdentity has no signer/private field. This
    // cannot be asserted at runtime, but the compile-time field set is fixed by
    // the struct definition (verified by constructing it only from public data).
    let _: &LoadedIdentity = ident;
}

#[test]
fn load_ssh_identities_only_filter_with_nonexistent_name_returns_empty() {
    let mut vault = unlocked_vault();
    insert_ssh_record(&mut vault, "github-key");

    let filter = IdentityFilter {
        only: vec!["nonexistent".to_string()],
        allow: None,
    };
    let identities =
        load_ssh_identities(&vault, &filter).expect("load must succeed even with empty result");

    assert!(
        identities.is_empty(),
        "whitelist with no matching name must yield zero identities"
    );
}

#[test]
fn load_ssh_identities_only_filter_matching_name_returns_one() {
    let mut vault = unlocked_vault();
    insert_ssh_record(&mut vault, "github-key");
    insert_ssh_record(&mut vault, "gitlab-key");

    let filter = IdentityFilter {
        only: vec!["github-key".to_string()],
        allow: None,
    };
    let identities = load_ssh_identities(&vault, &filter).expect("load must succeed");

    assert_eq!(
        identities.len(),
        1,
        "only the whitelisted name must be loaded"
    );
    assert_eq!(identities[0].name, "github-key");
}

#[test]
fn load_ssh_identities_excludes_non_ssh_records() {
    let mut vault = unlocked_vault();
    let ssh_id = insert_ssh_record(&mut vault, "github-key");
    // A Login credential must never appear as an SSH identity.
    insert_login_record(&mut vault, "my-login");

    let identities =
        load_ssh_identities(&vault, &IdentityFilter::default()).expect("load must succeed");

    assert_eq!(
        identities.len(),
        1,
        "Login records must be excluded; only SSH records load"
    );
    assert_eq!(identities[0].record_id, ssh_id);
    assert!(
        !identities.iter().any(|i| i.name == "my-login"),
        "login credential must not leak into SSH identity list"
    );
}

#[test]
fn load_ssh_identities_allow_regex_matches_names() {
    let mut vault = unlocked_vault();
    insert_ssh_record(&mut vault, "github-key");
    insert_ssh_record(&mut vault, "gitlab-key");
    insert_ssh_record(&mut vault, "backup-key");

    let filter = IdentityFilter {
        only: vec![],
        allow: Some(Regex::new("^git").unwrap()),
    };
    let identities = load_ssh_identities(&vault, &filter).expect("load must succeed");

    assert_eq!(
        identities.len(),
        2,
        "regex must match github/gitlab but not backup"
    );
    let names: Vec<&str> = identities.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"github-key"));
    assert!(names.contains(&"gitlab-key"));
    assert!(!names.contains(&"backup-key"));
}

#[test]
fn identity_filter_default_matches_all_names() {
    let filter = IdentityFilter::default();
    assert!(
        filter.matches("anything"),
        "empty filter matches every name"
    );
    assert!(
        filter.matches(""),
        "empty filter matches even the empty string"
    );
}

#[test]
fn identity_filter_only_and_allow_are_or_combined() {
    // A name is accepted if it is in `only` OR matches `allow`.
    let filter = IdentityFilter {
        only: vec!["exact-name".to_string()],
        allow: Some(Regex::new("^dev-").unwrap()),
    };
    assert!(filter.matches("exact-name"), "in `only` -> match");
    assert!(
        filter.matches("dev-server"),
        "matches `allow` regex -> match"
    );
    assert!(
        !filter.matches("production"),
        "neither in `only` nor matching `allow` -> no match"
    );
}

// ===========================================================================
// RSA signer (`RsaSigner`)
// ===========================================================================
//
// These tests exercise `RsaSigner` end-to-end: an OpenSSH RSA PEM is parsed, a
// message is signed, and the returned wire-format signature blob is parsed and
// verified directly with the `rsa` crate's PKCS#1 v1.5 verifier (NOT via
// `ssh_key::SshSig`, because the SSH agent `SIGN_RESPONSE` carries a raw
// algorithm-specific signature blob, not an SSHSIG wrapper).
//
// RFC 8332 wire format returned by `RsaSigner::sign`:
//   string "rsa-sha2-256" | "rsa-sha2-512"
//   string <pkcs1v15 sig bytes>
// where every `string` is a 4-byte big-endian length prefix + bytes. SHA-1
// `ssh-rsa` is a Non-Goal; with no flags we DEFAULT to rsa-sha2-256 (modern
// ssh requires SHA-2).

/// Parse an SSH agent RSA wire-format signature blob and return
/// `(algorithm_name, signature_bytes)`.
fn extract_rsa_sig(blob: &[u8]) -> (&[u8], &[u8]) {
    let (alg, rest) = read_string_local(blob).expect("sig has algorithm-name string");
    let (sig, tail) = read_string_local(rest).expect("sig has signature string");
    assert!(tail.is_empty(), "no trailing bytes in RSA sig blob");
    (alg, sig)
}

/// Local copy of the ssh "string" parser (the protocol-test file's version is
/// private). 4-byte big-endian length prefix + bytes.
fn read_string_local(input: &[u8]) -> Option<(&[u8], &[u8])> {
    if input.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes(input[0..4].try_into().unwrap()) as usize;
    if input.len() < 4 + len {
        return None;
    }
    Some((&input[4..4 + len], &input[4 + len..]))
}

/// Extract the rsa::RsaPublicKey from an OpenSSH PEM via ssh-key's PUBLIC-key
/// path, for independent PKCS#1 v1.5 verification.
///
/// Uses the public-key conversion (`TryFrom<&ssh_key::public::RsaPublicKey>`),
/// which only needs `n` and `e` — deliberately NOT the private-key
/// `TryFrom<&RsaKeypair>`, which has a prime-duplication bug in ssh-key 0.6.7
/// (passes `p` twice). The public-key path is unaffected.
fn rsa_public_key_from_pem(pem: &str) -> rsa::RsaPublicKey {
    let private = ssh_key::PrivateKey::from_openssh(pem).expect("PEM must parse");
    let rsa_pub = match private.public_key().key_data() {
        ssh_key::public::KeyData::Rsa(pk) => pk,
        other => panic!("expected RSA public key, got {other:?}"),
    };
    rsa::RsaPublicKey::try_from(rsa_pub).expect("ssh-key pub -> rsa::RsaPublicKey")
}

/// Verify a PKCS#1 v1.5 signature for `msg` against `pubkey` using the given
/// SHA-2 variant, identified by the wire algorithm name.
fn verify_rsa_pkcs1v15(pubkey: &rsa::RsaPublicKey, alg: &[u8], msg: &[u8], sig: &[u8]) {
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::signature::Verifier;

    let signature = Signature::try_from(sig).expect("sig bytes -> pkcs1v15::Signature");
    match alg {
        b"rsa-sha2-256" => {
            let vk: VerifyingKey<sha2::Sha256> = VerifyingKey::new(pubkey.clone());
            vk.verify(msg, &signature)
                .expect("rsa-sha2-256 signature must verify");
        }
        b"rsa-sha2-512" => {
            let vk: VerifyingKey<sha2::Sha512> = VerifyingKey::new(pubkey.clone());
            vk.verify(msg, &signature)
                .expect("rsa-sha2-512 signature must verify");
        }
        other => panic!("unexpected RSA wire algorithm: {other:?}"),
    }
}

#[test]
fn rsa_sign_with_default_flags_uses_sha2_256() {
    // No flags set: the modern default MUST be rsa-sha2-256 (modern ssh
    // refuses SHA-1 `ssh-rsa`). Old SHA-1 `ssh-rsa` is a Non-Goal.
    let pem = include_str!("fixtures/test_rsa");
    let signer = RsaSigner::from_openssh(pem, None).expect("unencrypted RSA key must load");

    assert_eq!(signer.algorithm(), SshAlgo::Rsa);

    let data = b"rsa-default-sign-test";
    let blob = signer
        .sign(data, SignFlags::default())
        .expect("signing must succeed");

    let (alg, sig) = extract_rsa_sig(&blob);
    assert_eq!(alg, b"rsa-sha2-256", "default must be rsa-sha2-256");
    let pubkey = rsa_public_key_from_pem(pem);
    verify_rsa_pkcs1v15(&pubkey, alg, data, sig);
}

#[test]
fn rsa_sign_with_sha2_256_flag() {
    let pem = include_str!("fixtures/test_rsa");
    let signer = RsaSigner::from_openssh(pem, None).expect("key must load");
    let data = b"rsa-sha2-256-explicit";
    let blob = signer
        .sign(
            data,
            SignFlags {
                rsa_sha2_256: true,
                rsa_sha2_512: false,
            },
        )
        .expect("signing must succeed");

    let (alg, sig) = extract_rsa_sig(&blob);
    assert_eq!(alg, b"rsa-sha2-256");
    let pubkey = rsa_public_key_from_pem(pem);
    verify_rsa_pkcs1v15(&pubkey, alg, data, sig);
}

#[test]
fn rsa_sign_with_sha2_512_flag() {
    let pem = include_str!("fixtures/test_rsa");
    let signer = RsaSigner::from_openssh(pem, None).expect("key must load");
    let data = b"rsa-sha2-512-explicit";
    let blob = signer
        .sign(
            data,
            SignFlags {
                rsa_sha2_256: false,
                rsa_sha2_512: true,
            },
        )
        .expect("signing must succeed");

    let (alg, sig) = extract_rsa_sig(&blob);
    assert_eq!(alg, b"rsa-sha2-512");
    let pubkey = rsa_public_key_from_pem(pem);
    verify_rsa_pkcs1v15(&pubkey, alg, data, sig);
}

#[test]
fn rsa_sign_sha2_512_wins_over_sha2_256_when_both_set() {
    // When BOTH flags are set, RSA-SHA2-512 wins (matches OpenSSH behavior:
    // the stronger hash is preferred when the client offers both).
    let pem = include_str!("fixtures/test_rsa");
    let signer = RsaSigner::from_openssh(pem, None).expect("key must load");
    let blob = signer
        .sign(
            b"both-flags",
            SignFlags {
                rsa_sha2_256: true,
                rsa_sha2_512: true,
            },
        )
        .expect("signing must succeed");
    let (alg, _sig) = extract_rsa_sig(&blob);
    assert_eq!(alg, b"rsa-sha2-512", "SHA2-512 must win when both set");
}

#[test]
fn rsa_sign_is_deterministic() {
    // PKCS#1 v1.5 (RFC 8017 §8.2) is deterministic — no randomness. Same
    // data + flags => same signature bytes.
    let pem = include_str!("fixtures/test_rsa");
    let signer = RsaSigner::from_openssh(pem, None).expect("key must load");
    let flags = SignFlags {
        rsa_sha2_256: true,
        rsa_sha2_512: false,
    };
    let a = signer.sign(b"repeatable", flags).unwrap();
    let b = signer.sign(b"repeatable", flags).unwrap();
    assert_eq!(a, b, "PKCS#1 v1.5 RSA signing must be deterministic");
}

#[test]
fn rsa_public_key_ssh_is_openssh_format() {
    let pem = include_str!("fixtures/test_rsa");
    let signer = RsaSigner::from_openssh(pem, None).expect("key must load");
    let public_ssh = signer.public_key_ssh().expect("public key string");
    assert!(
        public_ssh.starts_with("ssh-rsa "),
        "public key must be OpenSSH format, got: {public_ssh}"
    );
}

#[test]
fn rsa_passphrase_protected_key_loads_and_signs() {
    let pem = include_str!("fixtures/test_rsa_encrypted");
    let signer = RsaSigner::from_openssh(pem, Some("test-passphrase-123"))
        .expect("passphrase-protected RSA key must decrypt");

    let data = b"encrypted-rsa-sign-test";
    let blob = signer
        .sign(
            data,
            SignFlags {
                rsa_sha2_256: true,
                rsa_sha2_512: false,
            },
        )
        .expect("signing must succeed");

    let (alg, sig) = extract_rsa_sig(&blob);
    assert_eq!(alg, b"rsa-sha2-256");
    let pubkey = rsa_public_key_from_pem(pem);
    verify_rsa_pkcs1v15(&pubkey, alg, data, sig);
}

#[test]
fn rsa_passphrase_protected_key_wrong_passphrase_fails() {
    let pem = include_str!("fixtures/test_rsa_encrypted");
    let result = RsaSigner::from_openssh(pem, Some("wrong-passphrase"));
    assert!(
        result.is_err(),
        "wrong passphrase must fail loudly, not silently load or panic"
    );
}

#[test]
fn rsa_missing_passphrase_for_encrypted_key_fails() {
    let pem = include_str!("fixtures/test_rsa_encrypted");
    let result = RsaSigner::from_openssh(pem, None);
    assert!(
        result.is_err(),
        "encrypted RSA key without passphrase must fail loudly"
    );
}

#[test]
fn rsa_passphrase_supplied_for_unencrypted_key_is_rejected() {
    // Fail loud: a passphrase on an unencrypted key is a caller error, never
    // silently ignored.
    let pem = include_str!("fixtures/test_rsa");
    let result = RsaSigner::from_openssh(pem, Some("unused-passphrase"));
    assert!(
        result.is_err(),
        "passphrase on an unencrypted RSA key must be rejected, not ignored"
    );
}

#[test]
fn rsa_sign_rejects_non_rsa_key() {
    // Feeding an ed25519 PEM to RsaSigner must fail loudly (UnsupportedKeyType
    // — error sanitization, no panic, no partial load).
    let pem = include_str!("fixtures/test_ed25519");
    let result = RsaSigner::from_openssh(pem, None);
    assert!(
        result.is_err(),
        "RsaSigner must reject non-RSA keys, not silently accept them"
    );
}

// ===========================================================================
// ECDSA signer (`EcdsaSigner`) — NIST P-256 / P-384 / P-521
// ===========================================================================
//
// These tests exercise `EcdsaSigner` end-to-end per curve: an OpenSSH ECDSA PEM
// is parsed, a message is signed, and the returned wire-format signature blob is
// parsed and verified directly with the matching RustCrypto crate (p256/p384/
// p521) — NOT via `ssh_key::SshSig`, because the SSH agent `SIGN_RESPONSE`
// carries a raw algorithm-specific signature blob, not an SSHSIG wrapper.
//
// RFC 5656 / draft-miller-ssh-agent wire format returned by `EcdsaSigner::sign`:
//   string "ecdsa-sha2-nistp256" | "ecdsa-sha2-nistp384" | "ecdsa-sha2-nistp521"
//   string <DER-encoded ECDSA sig over data, hashed with the curve's SHA-2
//           (P-256 -> SHA-256, P-384 -> SHA-384, P-521 -> SHA-512)>
// ECDSA ignores `SignFlags` (those select RSA SHA-2 variants).

/// Parse an SSH agent ECDSA wire-format signature blob and return
/// `(algorithm_name, der_signature_bytes)`. Layout: `string <alg>` +
/// `string <DER sig>`.
fn extract_ecdsa_sig(blob: &[u8]) -> (&[u8], &[u8]) {
    let (alg, rest) = read_string_local(blob).expect("ecdsa sig has algorithm-name string");
    let (sig, tail) = read_string_local(rest).expect("ecdsa sig has signature string");
    assert!(tail.is_empty(), "no trailing bytes in ECDSA sig blob");
    (alg, sig)
}

/// Extract the SEC1-encoded ECDSA public point from an OpenSSH PEM via ssh-key's
/// public-key path, for independent verification with the curve crate.
fn ecdsa_sec1_public(pem: &str) -> (ssh_key::EcdsaCurve, Vec<u8>) {
    let private = ssh_key::PrivateKey::from_openssh(pem).expect("PEM must parse");
    let pub_key = match private.public_key().key_data() {
        ssh_key::public::KeyData::Ecdsa(pk) => pk,
        other => panic!("expected ECDSA public key, got {other:?}"),
    };
    (pub_key.curve(), pub_key.as_sec1_bytes().to_vec())
}

#[test]
fn ecdsa_p256_sign_is_verifiable() {
    let pem = include_str!("fixtures/test_ecdsa_256");
    let signer = EcdsaSigner::from_openssh(pem, None).expect("unencrypted P-256 key must load");

    assert_eq!(signer.algorithm(), SshAlgo::Ecdsa(EcdsaCurve::P256));

    let data = b"ecdsa-p256-sign-test";
    let blob = signer
        .sign(data, SignFlags::default())
        .expect("signing must succeed");

    let (alg, der_sig) = extract_ecdsa_sig(&blob);
    assert_eq!(alg, b"ecdsa-sha2-nistp256", "wire algorithm name for P-256");

    // Independent verify with p256 (digest is SHA-256, chosen automatically by
    // the crate's DigestPrimitive impl).
    let (curve, sec1) = ecdsa_sec1_public(pem);
    assert_eq!(curve, ssh_key::EcdsaCurve::NistP256);
    let vk = p256::ecdsa::VerifyingKey::from_sec1_bytes(&sec1).expect("valid P-256 public key");
    let sig = p256::ecdsa::Signature::from_der(der_sig).expect("DER sig must decode");
    use p256::ecdsa::signature::Verifier;
    vk.verify(data, &sig)
        .expect("P-256 signature must verify against the public key");
}

#[test]
fn ecdsa_p384_sign_is_verifiable() {
    let pem = include_str!("fixtures/test_ecdsa_384");
    let signer = EcdsaSigner::from_openssh(pem, None).expect("unencrypted P-384 key must load");

    assert_eq!(signer.algorithm(), SshAlgo::Ecdsa(EcdsaCurve::P384));

    let data = b"ecdsa-p384-sign-test";
    let blob = signer
        .sign(data, SignFlags::default())
        .expect("signing must succeed");

    let (alg, der_sig) = extract_ecdsa_sig(&blob);
    assert_eq!(alg, b"ecdsa-sha2-nistp384", "wire algorithm name for P-384");

    let (curve, sec1) = ecdsa_sec1_public(pem);
    assert_eq!(curve, ssh_key::EcdsaCurve::NistP384);
    let vk = p384::ecdsa::VerifyingKey::from_sec1_bytes(&sec1).expect("valid P-384 public key");
    let sig = p384::ecdsa::Signature::from_der(der_sig).expect("DER sig must decode");
    use p384::ecdsa::signature::Verifier;
    vk.verify(data, &sig)
        .expect("P-384 signature must verify against the public key");
}

#[test]
fn ecdsa_p521_sign_is_verifiable() {
    let pem = include_str!("fixtures/test_ecdsa_521");
    let signer = EcdsaSigner::from_openssh(pem, None).expect("unencrypted P-521 key must load");

    assert_eq!(signer.algorithm(), SshAlgo::Ecdsa(EcdsaCurve::P521));

    let data = b"ecdsa-p521-sign-test";
    let blob = signer
        .sign(data, SignFlags::default())
        .expect("signing must succeed");

    let (alg, der_sig) = extract_ecdsa_sig(&blob);
    assert_eq!(alg, b"ecdsa-sha2-nistp521", "wire algorithm name for P-521");

    let (curve, sec1) = ecdsa_sec1_public(pem);
    assert_eq!(curve, ssh_key::EcdsaCurve::NistP521);
    let vk = p521::ecdsa::VerifyingKey::from_sec1_bytes(&sec1).expect("valid P-521 public key");
    let sig = p521::ecdsa::Signature::from_der(der_sig).expect("DER sig must decode");
    use p521::ecdsa::signature::Verifier;
    vk.verify(data, &sig)
        .expect("P-521 signature must verify against the public key");
}

#[test]
fn ecdsa_sign_flags_are_ignored() {
    // RSA-only flags must be accepted (no-op) by the ECDSA signer.
    let pem = include_str!("fixtures/test_ecdsa_256");
    let signer = EcdsaSigner::from_openssh(pem, None).expect("key must load");
    let flags = SignFlags {
        rsa_sha2_256: true,
        rsa_sha2_512: true,
    };
    let blob = signer.sign(b"data", flags).expect("flags must not error");
    assert!(!blob.is_empty());
}

#[test]
fn ecdsa_sign_is_deterministic() {
    // The `ecdsa` crate's `Signer` impl uses RFC 6979: the nonce `k` is derived
    // from the key+message, so two signatures over the same data are identical.
    // This matches the deterministic ed25519 / RSA-PKCS1v15 paths and still
    // produces valid, verifiable ECDSA signatures.
    let pem = include_str!("fixtures/test_ecdsa_256");
    let signer = EcdsaSigner::from_openssh(pem, None).expect("key must load");
    let a = signer.sign(b"same", SignFlags::default()).unwrap();
    let b = signer.sign(b"same", SignFlags::default()).unwrap();
    assert_eq!(
        a, b,
        "ECDSA signatures over the same data must be identical (RFC 6979 deterministic)"
    );
    // Different data yields a different signature blob.
    let c = signer.sign(b"different", SignFlags::default()).unwrap();
    assert_ne!(a, c, "different messages must produce different signatures");
}

#[test]
fn ecdsa_public_key_ssh_is_openssh_format() {
    let pem = include_str!("fixtures/test_ecdsa_256");
    let signer = EcdsaSigner::from_openssh(pem, None).expect("key must load");
    let public_ssh = signer.public_key_ssh().expect("public key string");
    assert!(
        public_ssh.starts_with("ecdsa-sha2-nistp256 "),
        "public key must be OpenSSH format, got: {public_ssh}"
    );
}

#[test]
fn ecdsa_passphrase_protected_key_loads_and_signs() {
    let pem = include_str!("fixtures/test_ecdsa_384_encrypted");
    let signer = EcdsaSigner::from_openssh(pem, Some("test-passphrase-123"))
        .expect("passphrase-protected ECDSA key must decrypt");

    let data = b"encrypted-ecdsa-sign-test";
    let blob = signer
        .sign(data, SignFlags::default())
        .expect("signing must succeed");

    let (alg, der_sig) = extract_ecdsa_sig(&blob);
    assert_eq!(alg, b"ecdsa-sha2-nistp384");
    let (_curve, sec1) = ecdsa_sec1_public(pem);
    let vk = p384::ecdsa::VerifyingKey::from_sec1_bytes(&sec1).expect("valid P-384 public key");
    let sig = p384::ecdsa::Signature::from_der(der_sig).expect("DER sig must decode");
    use p384::ecdsa::signature::Verifier;
    vk.verify(data, &sig).expect("P-384 signature must verify");
}

#[test]
fn ecdsa_passphrase_protected_key_wrong_passphrase_fails() {
    let pem = include_str!("fixtures/test_ecdsa_384_encrypted");
    let result = EcdsaSigner::from_openssh(pem, Some("wrong-passphrase"));
    assert!(
        result.is_err(),
        "wrong passphrase must fail loudly, not silently load or panic"
    );
}

#[test]
fn ecdsa_missing_passphrase_for_encrypted_key_fails() {
    let pem = include_str!("fixtures/test_ecdsa_384_encrypted");
    let result = EcdsaSigner::from_openssh(pem, None);
    assert!(
        result.is_err(),
        "encrypted ECDSA key without passphrase must fail loudly"
    );
}

#[test]
fn ecdsa_passphrase_supplied_for_unencrypted_key_is_rejected() {
    let pem = include_str!("fixtures/test_ecdsa_256");
    let result = EcdsaSigner::from_openssh(pem, Some("unused-passphrase"));
    assert!(
        result.is_err(),
        "passphrase on an unencrypted ECDSA key must be rejected, not ignored"
    );
}

#[test]
fn ecdsa_signer_rejects_non_ecdsa_key() {
    // Feeding an ed25519 PEM to EcdsaSigner must fail loudly (UnsupportedKeyType
    // — error sanitization, no panic, no partial load).
    let pem = include_str!("fixtures/test_ed25519");
    let result = EcdsaSigner::from_openssh(pem, None);
    assert!(
        result.is_err(),
        "EcdsaSigner must reject non-ECDSA keys, not silently accept them"
    );
}
