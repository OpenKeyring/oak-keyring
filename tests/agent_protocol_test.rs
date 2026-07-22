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
