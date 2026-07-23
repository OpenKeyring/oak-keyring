//! Integration tests for the SSH agent wire-protocol server
//! (`oak_keyring::agent::server::AgentServer`).
//!
//! These tests drive the server with a hand-rolled ssh-agent *client* (no
//! external ssh-agent crate): they speak the wire protocol directly over the
//! Unix socket — `REQUEST_IDENTITIES` (11) and `SIGN_REQUEST` (13) — and
//! verify both the identity listing and the end-to-end sign roundtrip against
//! the vault-stored ed25519 key.
//!
//! Wire protocol (draft-ietf-miller-ssh-agent): every message is a 4-byte
//! big-endian length prefix followed by a payload whose first byte is the
//! message type.
//!
//! ZERO-CACHE contract: the server must not retain a signer or private key
//! across requests. These tests assert the observable behavior — identities
//! list only public blobs, and signing works without any pre-loaded signer —
//! which is the externally verifiable part of that contract.

use std::path::{Path, PathBuf};
use std::time::Duration;

use oak_keyring::agent::identity::IdentityFilter;
use oak_keyring::agent::server::AgentServer;
use oak_keyring::crypto::bip39::{MnemonicLanguage, Passkey};
use oak_keyring::db::schema::init_db_in_memory;
use oak_keyring::services::vault::VaultService;
use oak_keyring::types::credential::{CredentialType, EncryptedPayload};
use oak_keyring::types::record::CreateRecordParams;
use oak_keyring::types::sensitive::SecureStr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// A real unencrypted ed25519 OpenSSH private key (matches `fixtures/test_ed25519.pub`).
const ED25519_PEM: &str = include_str!("fixtures/test_ed25519");

/// The OpenSSH public key string for `ED25519_PEM`, stored as the vault record's
/// `public_key` field so the server can parse it into a wire-format blob.
const ED25519_PUB_SSH: &str = "ssh-ed25519 \
     AAAAC3NzaC1lZDI1NTE5AAAAIPddLwxmYUz+k43Vr+cahIy1iOROowugaJr8lQ6Tmi2V \
     test-ed25519@oak-keyring";

// ── ssh-agent wire message types ─────────────────────────────────────────────
const SSH_AGENT_FAILURE: u8 = 5;
// const SSH_AGENT_SUCCESS: u8 = 6;
const SSH_AGENTC_REQUEST_IDENTITIES: u8 = 11;
const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
const SSH_AGENTC_SIGN_REQUEST: u8 = 13;
const SSH_AGENT_SIGN_RESPONSE: u8 = 14;
/// Sign-request flag bit requesting `rsa-sha2-256` (RFC 8332). RSA-only.
const SSH_AGENT_RSA_SHA2_256: u32 = 0x02;
/// Sign-request flag bit requesting `rsa-sha2-512` (RFC 8332). RSA-only.
const SSH_AGENT_RSA_SHA2_512: u32 = 0x04;

// ===========================================================================
// client-side wire codec helpers
// ===========================================================================

/// Append an ssh "string" (4-byte big-endian length prefix + bytes) to `out`.
fn write_string(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

/// Parse an ssh "string" from the front of `input`, returning `(value, rest)`.
fn read_string(input: &[u8]) -> Option<(&[u8], &[u8])> {
    if input.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes(input[0..4].try_into().unwrap()) as usize;
    if input.len() < 4 + len {
        return None;
    }
    Some((&input[4..4 + len], &input[4 + len..]))
}

/// Write a framed message: 4-byte big-endian payload length + payload.
async fn write_frame(stream: &mut UnixStream, payload: &[u8]) -> std::io::Result<()> {
    stream
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await?;
    stream.write_all(payload).await?;
    stream.flush().await
}

/// Read one framed message payload (the bytes after the 4-byte length prefix).
async fn read_frame(stream: &mut UnixStream) -> std::io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;
    Ok(payload)
}

/// Send one agent request and read one response payload.
async fn agent_round_trip(socket_path: &Path, request: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut stream = connect_with_retry(socket_path).await;
    write_frame(&mut stream, request).await?;
    read_frame(&mut stream).await
}

/// Connect to the server socket, retrying briefly while it comes up.
async fn connect_with_retry(socket_path: &Path) -> UnixStream {
    for _ in 0..100 {
        match UnixStream::connect(socket_path).await {
            Ok(s) => return s,
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
    panic!("agent server did not come up at {}", socket_path.display());
}

/// Build a unique temp socket path, scoped to a TempDir for auto-cleanup.
fn temp_socket_path() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = dir.path().join("agent.sock");
    (dir, path)
}

/// Build an unlocked in-memory vault holding one ed25519 SSH record with the
/// real private key stored, returning `(vault, record_id)`.
fn unlocked_vault_with_ed25519() -> (VaultService, uuid::Uuid) {
    let conn = init_db_in_memory().expect("in-memory db");
    let mut svc = VaultService::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
    svc.unlock_with_mnemonic(&mnemonic)
        .expect("unlock_with_mnemonic must succeed in test");

    let id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Ssh,
            payload: EncryptedPayload::Ssh {
                name: "github-key".to_string(),
                public_key: ED25519_PUB_SSH.to_string(),
                private_key: Some(SecureStr::new(ED25519_PEM.to_string())),
                passphrase: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create ssh record");
    (svc, id)
}

/// Spawn the agent server bound at `socket_path`, returning the join handle.
fn spawn_server(
    vault: VaultService,
    socket_path: PathBuf,
) -> tokio::task::JoinHandle<Result<(), oak_keyring::agent::server::AgentServerError>> {
    let server = AgentServer::start(vault, IdentityFilter::default(), socket_path)
        .expect("agent server must start");
    tokio::spawn(async move { server.serve().await })
}

/// Extract the 32-byte ed25519 public key from a wire-format public blob.
fn ed25519_pubkey_from_blob(blob: &[u8]) -> [u8; 32] {
    let (alg, rest) = read_string(blob).expect("blob has algorithm-name string");
    assert_eq!(alg, b"ssh-ed25519", "blob must be an ed25519 key");
    let (key, tail) = read_string(rest).expect("blob has public-key string");
    assert!(tail.is_empty(), "no trailing bytes in public blob");
    assert_eq!(key.len(), 32, "ed25519 public key is 32 bytes");
    key.try_into().unwrap()
}

/// Parse an ssh-agent ed25519 wire-format signature blob and return the raw
/// 64-byte signature. Layout: `string "ssh-ed25519" + string <64-byte sig>`.
fn extract_ed25519_sig(blob: &[u8]) -> &[u8] {
    let (alg, rest) = read_string(blob).expect("sig has algorithm-name string");
    assert_eq!(alg, b"ssh-ed25519");
    let (sig, tail) = read_string(rest).expect("sig has signature string");
    assert_eq!(sig.len(), 64, "ed25519 signature is 64 bytes");
    assert!(tail.is_empty(), "no trailing bytes in sig blob");
    sig
}

// ===========================================================================
// tests
// ===========================================================================

#[tokio::test]
async fn identities_and_sign_roundtrip() {
    let (_dir, sock) = temp_socket_path();
    let (vault, _ssh_id) = unlocked_vault_with_ed25519();
    let handle = spawn_server(vault, sock.clone());

    // ── REQUEST_IDENTITIES ───────────────────────────────────────────────
    let resp = agent_round_trip(&sock, &[SSH_AGENTC_REQUEST_IDENTITIES])
        .await
        .expect("identities round trip");

    assert_eq!(
        resp[0], SSH_AGENT_IDENTITIES_ANSWER,
        "REQUEST_IDENTITIES must be answered with IDENTITIES_ANSWER"
    );
    let count = u32::from_be_bytes(resp[1..5].try_into().unwrap());
    assert_eq!(count, 1, "exactly one identity must be advertised");

    let (blob, rest) = read_string(&resp[5..]).expect("identity blob string");
    let (comment, tail) = read_string(rest).expect("identity comment string");
    assert!(tail.is_empty(), "no trailing bytes after the identity");
    assert_eq!(
        comment, b"github-key",
        "comment must be the vault record name"
    );
    // The blob is an ed25519 public key.
    let pub_bytes = ed25519_pubkey_from_blob(blob);

    // ── SIGN_REQUEST with that blob ──────────────────────────────────────
    let data = b"authenticate me, agent";
    let mut sign_req = Vec::new();
    sign_req.push(SSH_AGENTC_SIGN_REQUEST);
    write_string(&mut sign_req, blob);
    write_string(&mut sign_req, data);
    sign_req.extend_from_slice(&0u32.to_be_bytes()); // flags = 0 (ed25519 ignores flags)

    let sign_resp = agent_round_trip(&sock, &sign_req)
        .await
        .expect("sign round trip");
    assert_eq!(
        sign_resp[0], SSH_AGENT_SIGN_RESPONSE,
        "SIGN_REQUEST must be answered with SIGN_RESPONSE"
    );
    let (sig_blob, tail) = read_string(&sign_resp[1..]).expect("signature string");
    assert!(tail.is_empty(), "no trailing bytes after the signature");

    // ── verify the signature against the public key (independent) ────────
    let sig_bytes = extract_ed25519_sig(sig_blob);
    let sig = ed25519_dalek::Signature::from_bytes(sig_bytes.try_into().unwrap());
    let vk = ed25519_dalek::VerifyingKey::from_bytes(&pub_bytes).expect("valid public key");
    use ed25519_dalek::Verifier;
    vk.verify(data, &sig)
        .expect("the agent-produced signature must verify against the stored public key");

    handle.abort();
}

#[tokio::test]
async fn sign_request_with_unknown_blob_returns_failure() {
    let (_dir, sock) = temp_socket_path();
    let (vault, _) = unlocked_vault_with_ed25519();
    let handle = spawn_server(vault, sock.clone());

    // A blob that the server never advertised.
    let fake_blob = b"ssh-ed25519\0not-a-real-key";
    let mut sign_req = Vec::new();
    sign_req.push(SSH_AGENTC_SIGN_REQUEST);
    write_string(&mut sign_req, fake_blob);
    write_string(&mut sign_req, b"data");
    sign_req.extend_from_slice(&0u32.to_be_bytes());

    let resp = agent_round_trip(&sock, &sign_req)
        .await
        .expect("round trip");
    assert_eq!(
        resp[0], SSH_AGENT_FAILURE,
        "an unknown/unauthorized key blob must yield SSH_AGENT_FAILURE, not a signature"
    );

    handle.abort();
}

/// Multiple requests over a single connection must all be answered. This is
/// the real-ssh pattern: `ssh` opens ONE agent socket and issues
/// REQUEST_IDENTITIES followed by SIGN_REQUEST (often several) on it. The
/// server must loop reading frames from one `UnixStream` until clean EOF, not
/// drop the stream after the first reply.
#[tokio::test]
async fn multiple_requests_on_single_connection() {
    let (_dir, sock) = temp_socket_path();
    let (vault, _) = unlocked_vault_with_ed25519();
    let handle = spawn_server(vault, sock.clone());

    // One connection, reused for several requests.
    let mut stream = connect_with_retry(&sock).await;

    // ── First request: REQUEST_IDENTITIES ───────────────────────────────
    write_frame(&mut stream, &[SSH_AGENTC_REQUEST_IDENTITIES])
        .await
        .expect("write identities request");
    let resp = read_frame(&mut stream)
        .await
        .expect("read identities reply on the same connection");
    assert_eq!(
        resp[0], SSH_AGENT_IDENTITIES_ANSWER,
        "REQUEST_IDENTITIES must be answered with IDENTITIES_ANSWER"
    );
    let count = u32::from_be_bytes(resp[1..5].try_into().unwrap());
    assert_eq!(count, 1, "exactly one identity must be advertised");
    let (blob, rest) = read_string(&resp[5..]).expect("identity blob string");
    let (_comment, tail) = read_string(rest).expect("identity comment string");
    assert!(tail.is_empty(), "no trailing bytes after the identity");
    let pub_bytes = ed25519_pubkey_from_blob(blob);

    // ── Second request: SIGN_REQUEST with the blob from above, on the SAME
    //    socket. This is the sequence real ssh performs.
    let data = b"sign me over the same connection";
    let mut sign_req = Vec::new();
    sign_req.push(SSH_AGENTC_SIGN_REQUEST);
    write_string(&mut sign_req, blob);
    write_string(&mut sign_req, data);
    sign_req.extend_from_slice(&0u32.to_be_bytes()); // flags = 0

    write_frame(&mut stream, &sign_req)
        .await
        .expect("write sign request on the same connection");
    let sign_resp = read_frame(&mut stream)
        .await
        .expect("read sign reply on the same connection");
    assert_eq!(
        sign_resp[0], SSH_AGENT_SIGN_RESPONSE,
        "SIGN_REQUEST must be answered with SIGN_RESPONSE"
    );
    let (sig_blob, tail) = read_string(&sign_resp[1..]).expect("signature string");
    assert!(tail.is_empty(), "no trailing bytes after the signature");

    // Verify the signature against the public key (independent of the server).
    let sig_bytes = extract_ed25519_sig(sig_blob);
    let sig = ed25519_dalek::Signature::from_bytes(sig_bytes.try_into().unwrap());
    let vk = ed25519_dalek::VerifyingKey::from_bytes(&pub_bytes).expect("valid public key");
    use ed25519_dalek::Verifier;
    vk.verify(data, &sig)
        .expect("the agent-produced signature must verify against the stored public key");

    // ── Third request: another REQUEST_IDENTITIES, still on the same socket,
    //    proving the connection is still alive after the sign roundtrip.
    write_frame(&mut stream, &[SSH_AGENTC_REQUEST_IDENTITIES])
        .await
        .expect("write second identities request");
    let resp2 = read_frame(&mut stream)
        .await
        .expect("read second identities reply on the same connection");
    assert_eq!(resp2[0], SSH_AGENT_IDENTITIES_ANSWER);
    let count2 = u32::from_be_bytes(resp2[1..5].try_into().unwrap());
    assert_eq!(
        count2, 1,
        "connection must still serve requests after signing"
    );

    handle.abort();
}

/// A bad/unknown request must yield `SSH_AGENT_FAILURE` WITHOUT tearing down
/// the connection: the next request on the same socket must still succeed.
#[tokio::test]
async fn bad_request_does_not_close_connection() {
    let (_dir, sock) = temp_socket_path();
    let (vault, _) = unlocked_vault_with_ed25519();
    let handle = spawn_server(vault, sock.clone());

    let mut stream = connect_with_retry(&sock).await;

    // Unknown message type -> FAILURE, but the connection must stay open.
    write_frame(&mut stream, &[99u8])
        .await
        .expect("write unknown request");
    let resp = read_frame(&mut stream)
        .await
        .expect("read failure reply on the same connection");
    assert_eq!(
        resp[0], SSH_AGENT_FAILURE,
        "an unknown request must yield SSH_AGENT_FAILURE"
    );

    // The next request on the SAME connection must still succeed.
    write_frame(&mut stream, &[SSH_AGENTC_REQUEST_IDENTITIES])
        .await
        .expect("write identities request after a bad request");
    let resp2 = read_frame(&mut stream)
        .await
        .expect("connection must still be alive after a bad request");
    assert_eq!(
        resp2[0], SSH_AGENT_IDENTITIES_ANSWER,
        "connection must survive a single bad request"
    );

    handle.abort();
}

#[tokio::test]
async fn request_identities_applies_identity_filter() {
    let (_dir, sock) = temp_socket_path();

    // Build a vault with TWO ssh records; filter to only one.
    let (mut vault, _) = unlocked_vault_with_ed25519();
    vault
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Ssh,
            payload: EncryptedPayload::Ssh {
                name: "gitlab-key".to_string(),
                public_key: ED25519_PUB_SSH.to_string(),
                private_key: Some(SecureStr::new(ED25519_PEM.to_string())),
                passphrase: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create second ssh record");

    let filter = IdentityFilter {
        only: vec!["github-key".to_string()],
        allow: None,
    };
    let server = AgentServer::start(vault, filter, sock.clone()).expect("start");
    let handle = tokio::spawn(async move { server.serve().await });

    let resp = agent_round_trip(&sock, &[SSH_AGENTC_REQUEST_IDENTITIES])
        .await
        .expect("identities round trip");
    assert_eq!(resp[0], SSH_AGENT_IDENTITIES_ANSWER);
    let count = u32::from_be_bytes(resp[1..5].try_into().unwrap());
    assert_eq!(count, 1, "filter must restrict the advertised identities");

    handle.abort();
}

// ===========================================================================
// RSA protocol-level tests (RFC 8332 rsa-sha2-256 / rsa-sha2-512)
// ===========================================================================

/// A real unencrypted RSA 2048-bit OpenSSH private key (matches
/// `fixtures/test_rsa.pub`). Generated with `ssh-keygen -t rsa -b 2048`.
const RSA_PEM: &str = include_str!("fixtures/test_rsa");

/// The OpenSSH public key string for `RSA_PEM`, stored as the vault record's
/// `public_key` field so the server can parse it into a wire-format blob.
const RSA_PUB_SSH: &str = "ssh-rsa \
     AAAAB3NzaC1yc2EAAAADAQABAAABAQCvY1xq91xiyqmu52jAXBX3w9tgz1depXBwz3lJ6f6X3tMpyrkmPBRihrERDFIO3Oifehn+EzFo7Tt/EZ/Iuw9rYVll01Rm2biqRxEHsoCFPPxj3cryOPNTOW1YLw8kxFLqRtLntd51nToYjRt/+t4h5QrUWm/mkkQ8Ln5sac4DRlYqad1WzgKhnuwg5Wl3E1bAQK+d+ZIOZnvzYjCn3OuWL0iTgoPCNzQKFpqmYGzg2dpgaPnLkzvdF5mtQMdg7p9I0zwtvlf7oxqkv86Ggpctnz1ryEEgJqkqe9FxFp6CRImVgT8lOJerWJ8aVruX7KpR/jxT9c3oqd5OZJjOXmmV \
     test-rsa@oak-keyring";

/// Build an unlocked in-memory vault holding one RSA SSH record with the
/// real private key stored, returning `(vault, record_id)`.
fn unlocked_vault_with_rsa() -> (VaultService, uuid::Uuid) {
    let conn = init_db_in_memory().expect("in-memory db");
    let mut svc = VaultService::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
    svc.unlock_with_mnemonic(&mnemonic)
        .expect("unlock_with_mnemonic must succeed in test");

    let id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Ssh,
            payload: EncryptedPayload::Ssh {
                name: "rsa-deploy-key".to_string(),
                public_key: RSA_PUB_SSH.to_string(),
                private_key: Some(SecureStr::new(RSA_PEM.to_string())),
                passphrase: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create ssh record");
    (svc, id)
}

/// Extract the rsa::RsaPublicKey from the RSA PEM via the public-key path
/// (independent of the agent's signer, for end-to-end verification).
fn rsa_pubkey_from_pem(pem: &str) -> rsa::RsaPublicKey {
    let private = ssh_key::PrivateKey::from_openssh(pem).expect("RSA PEM must parse");
    let rsa_pub = match private.public_key().key_data() {
        ssh_key::public::KeyData::Rsa(pk) => pk,
        other => panic!("expected RSA public key, got {other:?}"),
    };
    rsa::RsaPublicKey::try_from(rsa_pub).expect("ssh-key pub -> rsa::RsaPublicKey")
}

/// Parse an ssh-agent RSA wire-format signature blob and return
/// `(algorithm_name, signature_bytes)`. Layout: `string <alg>` + `string <sig>`.
fn extract_rsa_sig(blob: &[u8]) -> (&[u8], &[u8]) {
    let (alg, rest) = read_string(blob).expect("RSA sig has algorithm-name string");
    let (sig, tail) = read_string(rest).expect("RSA sig has signature string");
    assert!(tail.is_empty(), "no trailing bytes in RSA sig blob");
    (alg, sig)
}

/// Verify an RSA PKCS#1 v1.5 signature against `pubkey` for `msg`, dispatching
/// on the wire algorithm name (`rsa-sha2-256` or `rsa-sha2-512`).
fn verify_rsa_sig(pubkey: &rsa::RsaPublicKey, alg: &[u8], msg: &[u8], sig: &[u8]) {
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::signature::Verifier;

    let signature = Signature::try_from(sig).expect("sig -> pkcs1v15::Signature");
    match alg {
        b"rsa-sha2-256" => {
            let vk: VerifyingKey<sha2::Sha256> = VerifyingKey::new(pubkey.clone());
            vk.verify(msg, &signature)
                .expect("rsa-sha2-256 signature must verify against the public key");
        }
        b"rsa-sha2-512" => {
            let vk: VerifyingKey<sha2::Sha512> = VerifyingKey::new(pubkey.clone());
            vk.verify(msg, &signature)
                .expect("rsa-sha2-512 signature must verify against the public key");
        }
        other => panic!("unexpected RSA wire algorithm from agent: {other:?}"),
    }
}

/// Drive a full RSA sign roundtrip over the agent socket: fetch the identity,
/// SIGN_REQUEST the RSA blob with `flags`, return `(alg, sig_bytes)` from the
/// agent's reply. Asserts the reply is a SIGN_RESPONSE (not FAILURE). Returns
/// owned `Vec<u8>` because the underlying reply buffer is local to this call.
async fn rsa_sign_roundtrip(sock: &Path, data: &[u8], flags: u32) -> (Vec<u8>, Vec<u8>) {
    // ── REQUEST_IDENTITIES ───────────────────────────────────────────────
    let resp = agent_round_trip(sock, &[SSH_AGENTC_REQUEST_IDENTITIES])
        .await
        .expect("identities round trip");
    assert_eq!(resp[0], SSH_AGENT_IDENTITIES_ANSWER);
    let count = u32::from_be_bytes(resp[1..5].try_into().unwrap());
    assert_eq!(count, 1, "exactly one RSA identity must be advertised");
    let (blob, rest) = read_string(&resp[5..]).expect("identity blob string");
    let (_comment, tail) = read_string(rest).expect("identity comment string");
    assert!(tail.is_empty());

    // The advertised blob must carry the ssh-rsa algorithm name.
    let (blob_alg, _) = read_string(blob).expect("blob has algorithm-name string");
    assert_eq!(
        blob_alg, b"ssh-rsa",
        "advertised blob must be an ssh-rsa key"
    );

    // ── SIGN_REQUEST with the blob and the requested flags ──────────────
    let mut sign_req = Vec::new();
    sign_req.push(SSH_AGENTC_SIGN_REQUEST);
    write_string(&mut sign_req, blob);
    write_string(&mut sign_req, data);
    sign_req.extend_from_slice(&flags.to_be_bytes());

    let sign_resp = agent_round_trip(sock, &sign_req)
        .await
        .expect("RSA sign round trip");
    assert_eq!(
        sign_resp[0], SSH_AGENT_SIGN_RESPONSE,
        "RSA SIGN_REQUEST must be answered with SIGN_RESPONSE"
    );
    let (sig_blob, tail) = read_string(&sign_resp[1..]).expect("signature string");
    assert!(tail.is_empty(), "no trailing bytes after the RSA signature");
    let (alg, sig) = extract_rsa_sig(sig_blob);
    (alg.to_vec(), sig.to_vec())
}

#[tokio::test]
async fn rsa_sign_request_with_sha2_256_flag_verifies() {
    let (_dir, sock) = temp_socket_path();
    let (vault, _rsa_id) = unlocked_vault_with_rsa();
    let handle = spawn_server(vault, sock.clone());

    let data = b"rsa-agent-sha2-256-roundtrip";
    let (alg, sig) = rsa_sign_roundtrip(&sock, data, SSH_AGENT_RSA_SHA2_256).await;
    assert_eq!(alg, b"rsa-sha2-256");

    let pubkey = rsa_pubkey_from_pem(RSA_PEM);
    verify_rsa_sig(&pubkey, &alg, data, &sig);

    handle.abort();
}

#[tokio::test]
async fn rsa_sign_request_with_sha2_512_flag_verifies() {
    let (_dir, sock) = temp_socket_path();
    let (vault, _rsa_id) = unlocked_vault_with_rsa();
    let handle = spawn_server(vault, sock.clone());

    let data = b"rsa-agent-sha2-512-roundtrip";
    let (alg, sig) = rsa_sign_roundtrip(&sock, data, SSH_AGENT_RSA_SHA2_512).await;
    assert_eq!(alg, b"rsa-sha2-512");

    let pubkey = rsa_pubkey_from_pem(RSA_PEM);
    verify_rsa_sig(&pubkey, &alg, data, &sig);

    handle.abort();
}

#[tokio::test]
async fn rsa_sign_request_with_no_flags_defaults_to_sha2_256() {
    // No SHA-2 flags set: the server must default to rsa-sha2-256 (modern ssh
    // refuses the legacy SHA-1 `ssh-rsa`). SHA-1 `ssh-rsa` support is a
    // spec Non-Goal.
    let (_dir, sock) = temp_socket_path();
    let (vault, _rsa_id) = unlocked_vault_with_rsa();
    let handle = spawn_server(vault, sock.clone());

    let data = b"rsa-agent-default-roundtrip";
    let (alg, sig) = rsa_sign_roundtrip(&sock, data, 0).await;
    assert_eq!(alg, b"rsa-sha2-256", "default must be rsa-sha2-256");

    let pubkey = rsa_pubkey_from_pem(RSA_PEM);
    verify_rsa_sig(&pubkey, &alg, data, &sig);

    handle.abort();
}

// ===========================================================================
// ECDSA protocol-level tests (RFC 5656 ecdsa-sha2-nistp256 / nistp384 /
// nistp521). ECDSA ignores the sign-request flags.
// ===========================================================================

/// A real unencrypted ECDSA P-256 OpenSSH private key (matches
/// `fixtures/test_ecdsa_256.pub`). Generated with `ssh-keygen -t ecdsa -b 256`.
const ECDSA_P256_PEM: &str = include_str!("fixtures/test_ecdsa_256");

/// OpenSSH public key string for `ECDSA_P256_PEM`, stored as the vault record's
/// `public_key` field so the server can parse it into a wire-format blob.
const ECDSA_P256_PUB_SSH: &str = "ecdsa-sha2-nistp256 \
     AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBBGa5aK5VjYmMz/7yIWmUYw82EsOumqQmqCcLI4Vxgs1hzMTR72rXnd4Cn1mnvNboaIlhwFTFVaBnWtBpIamkpw= \
     test-ecdsa-256@oak-keyring";

/// A real unencrypted ECDSA P-521 OpenSSH private key (matches
/// `fixtures/test_ecdsa_521.pub`). P-521 exercises the newly added identity
/// mapping and p521 signer.
const ECDSA_P521_PEM: &str = include_str!("fixtures/test_ecdsa_521");

/// OpenSSH public key string for `ECDSA_P521_PEM`.
const ECDSA_P521_PUB_SSH: &str = "ecdsa-sha2-nistp521 \
     AAAAE2VjZHNhLXNoYTItbmlzdHA1MjEAAAAIbmlzdHA1MjEAAACFBAArQaag1j8XYLrvIorPg40L8L4GddWeGuvI65y+FyNmepiZcH2++6F0qJz6/AnpCT5+Lnn5J5jOo+5gHdmIdyWOpQEh6zLOAa65AyG2zdfqmEdt3EULIWpbTOtXJtztosvNzJAOAVRr61FQrXgWtssZ/PtAsal9Xf+av1wH0+aXpSUQaA== \
     test-ecdsa-521@oak-keyring";

/// Build an unlocked in-memory vault holding one ECDSA SSH record with the
/// real private key stored, returning `(vault, record_id)`.
fn unlocked_vault_with_ecdsa(
    pem: &'static str,
    pub_ssh: &'static str,
    name: &str,
) -> (VaultService, uuid::Uuid) {
    let conn = init_db_in_memory().expect("in-memory db");
    let mut svc = VaultService::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
    svc.unlock_with_mnemonic(&mnemonic)
        .expect("unlock_with_mnemonic must succeed in test");

    let id = svc
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Ssh,
            payload: EncryptedPayload::Ssh {
                name: name.to_string(),
                public_key: pub_ssh.to_string(),
                private_key: Some(SecureStr::new(pem.to_string())),
                passphrase: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create ssh record");
    (svc, id)
}

/// Drive a full ECDSA sign roundtrip over the agent socket: fetch the identity,
/// SIGN_REQUEST its blob, and return `(blob, sig_bytes)` from the agent's reply.
/// Asserts the reply is a SIGN_RESPONSE (not FAILURE).
async fn ecdsa_sign_roundtrip(sock: &Path, data: &[u8]) -> (Vec<u8>, Vec<u8>) {
    // ── REQUEST_IDENTITIES ───────────────────────────────────────────────
    let resp = agent_round_trip(sock, &[SSH_AGENTC_REQUEST_IDENTITIES])
        .await
        .expect("identities round trip");
    assert_eq!(resp[0], SSH_AGENT_IDENTITIES_ANSWER);
    let count = u32::from_be_bytes(resp[1..5].try_into().unwrap());
    assert_eq!(count, 1, "exactly one ECDSA identity must be advertised");
    let (blob, rest) = read_string(&resp[5..]).expect("identity blob string");
    let (_comment, tail) = read_string(rest).expect("identity comment string");
    assert!(tail.is_empty());

    // ── SIGN_REQUEST (flags = 0; ECDSA ignores flags) ────────────────────
    let mut sign_req = Vec::new();
    sign_req.push(SSH_AGENTC_SIGN_REQUEST);
    write_string(&mut sign_req, blob);
    write_string(&mut sign_req, data);
    sign_req.extend_from_slice(&0u32.to_be_bytes());

    let sign_resp = agent_round_trip(sock, &sign_req)
        .await
        .expect("ECDSA sign round trip");
    assert_eq!(
        sign_resp[0], SSH_AGENT_SIGN_RESPONSE,
        "ECDSA SIGN_REQUEST must be answered with SIGN_RESPONSE"
    );
    let (sig_blob, tail) = read_string(&sign_resp[1..]).expect("signature string");
    assert!(
        tail.is_empty(),
        "no trailing bytes after the ECDSA signature"
    );
    (blob.to_vec(), sig_blob.to_vec())
}

#[tokio::test]
async fn ecdsa_p256_sign_request_verifies() {
    let (_dir, sock) = temp_socket_path();
    let (vault, _id) = unlocked_vault_with_ecdsa(ECDSA_P256_PEM, ECDSA_P256_PUB_SSH, "ecdsa-p256");
    let handle = spawn_server(vault, sock.clone());

    let data = b"ecdsa-agent-p256-roundtrip";
    let (blob, sig_blob) = ecdsa_sign_roundtrip(&sock, data).await;

    // The advertised blob must carry the ecdsa-sha2-nistp256 algorithm name.
    let (blob_alg, _) = read_string(&blob).expect("blob has algorithm-name string");
    assert_eq!(blob_alg, b"ecdsa-sha2-nistp256");

    // Parse the agent's signature blob: string alg + string <DER sig>.
    let (alg, der_sig) = read_string(&sig_blob).expect("sig has algorithm string");
    assert_eq!(alg, b"ecdsa-sha2-nistp256");
    let (der, tail) = read_string(der_sig).expect("sig has DER signature string");
    assert!(tail.is_empty());

    // Independent verify with p256 (digest SHA-256 chosen by the crate).
    let sec1 = ecdsa_sec1_public_from_pem(ECDSA_P256_PEM, ssh_key::EcdsaCurve::NistP256);
    let vk = p256::ecdsa::VerifyingKey::from_sec1_bytes(&sec1).expect("valid P-256 public key");
    let sig = p256::ecdsa::Signature::from_der(der).expect("DER sig must decode");
    use p256::ecdsa::signature::Verifier;
    vk.verify(data, &sig)
        .expect("P-256 agent signature must verify against the stored public key");

    handle.abort();
}

#[tokio::test]
async fn ecdsa_p521_sign_request_verifies() {
    // P-521 specifically exercises the newly added identity mapping (identity.rs
    // previously rejected NistP521) and the p521 signer/dispatch path.
    let (_dir, sock) = temp_socket_path();
    let (vault, _id) = unlocked_vault_with_ecdsa(ECDSA_P521_PEM, ECDSA_P521_PUB_SSH, "ecdsa-p521");
    let handle = spawn_server(vault, sock.clone());

    let data = b"ecdsa-agent-p521-roundtrip";
    let (blob, sig_blob) = ecdsa_sign_roundtrip(&sock, data).await;

    let (blob_alg, _) = read_string(&blob).expect("blob has algorithm-name string");
    assert_eq!(blob_alg, b"ecdsa-sha2-nistp521");

    let (alg, der_sig) = read_string(&sig_blob).expect("sig has algorithm string");
    assert_eq!(alg, b"ecdsa-sha2-nistp521");
    let (der, tail) = read_string(der_sig).expect("sig has DER signature string");
    assert!(tail.is_empty());

    // Independent verify with p521 (digest SHA-512 chosen by the crate).
    let sec1 = ecdsa_sec1_public_from_pem(ECDSA_P521_PEM, ssh_key::EcdsaCurve::NistP521);
    let vk = p521::ecdsa::VerifyingKey::from_sec1_bytes(&sec1).expect("valid P-521 public key");
    let sig = p521::ecdsa::Signature::from_der(der).expect("DER sig must decode");
    use p521::ecdsa::signature::Verifier;
    vk.verify(data, &sig)
        .expect("P-521 agent signature must verify against the stored public key");

    handle.abort();
}

/// Extract the SEC1 public point bytes from an OpenSSH PEM, asserting the curve
/// matches `expected` (defense against a fixture/constant mismatch).
fn ecdsa_sec1_public_from_pem(pem: &str, expected: ssh_key::EcdsaCurve) -> Vec<u8> {
    let private = ssh_key::PrivateKey::from_openssh(pem).expect("ECDSA PEM must parse");
    let pub_key = match private.public_key().key_data() {
        ssh_key::public::KeyData::Ecdsa(pk) => pk,
        other => panic!("expected ECDSA public key, got {other:?}"),
    };
    assert_eq!(pub_key.curve(), expected, "fixture curve mismatch");
    pub_key.as_sec1_bytes().to_vec()
}
