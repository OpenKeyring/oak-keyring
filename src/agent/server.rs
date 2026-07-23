//! SSH agent wire-protocol server over a Unix domain socket.
//!
//! Implements the message framing from [draft-ietf-miller-ssh-agent] by hand on
//! tokio's [`UnixListener`] (Option B from the Task 1 design: no `ssh-agent`
//! crate, whose tokio 0.1 / futures 0.1 pinning is incompatible with this
//! crate's runtime). Every wire message is a 4-byte big-endian length prefix
//! followed by a payload whose first byte is the message type.
//!
//! # Supported messages
//!
//! | Client request | Byte | Server reply | Byte |
//! |----------------|------|--------------|------|
//! | `REQUEST_IDENTITIES` | 11 | `IDENTITIES_ANSWER` | 12 |
//! | `SIGN_REQUEST` | 13 | `SIGN_RESPONSE` | 14 |
//! | anything else / error | — | `FAILURE` | 5 |
//!
//! # Zero-cache contract (RETENTION, not transient decrypt)
//!
//! [`AgentServer`] retains ONLY public data: the SSH wire-format public blobs,
//! record names, record ids, and the resolved algorithm. No signer and no
//! private key is kept across requests. The sign path decrypts the private key
//! material on demand (`decrypt_field_no_audit(Password)` for the key,
//! `decrypt_field_no_audit(Passphrase)` for its passphrase — the no-audit
//! variant because the agent writes its own single `SshSign` audit row from
//! [`handle_sign`], rather than a misleading `RecordViewPassword`), builds a
//! temporary signer, signs, and drops (zeroizes) the signer immediately.
//! Because the vault uses whole-payload AEAD, a field decrypt transiently
//! materializes the full payload plaintext in memory; the agent's guarantee is
//! that nothing is KEPT, not that the private key is never touched in memory.
//!
//! # Concurrency
//!
//! Each accepted connection is served to completion: an inner loop reads
//! frames from THAT connection until clean EOF or a read/write error,
//! potentially answering many requests over one file descriptor (the real-ssh
//! pattern — `ssh` issues `REQUEST_IDENTITIES` + `SIGN_REQUEST` over a single
//! agent socket). Connections themselves are handled sequentially in the
//! accept loop. The vault is accessed only in fully synchronous dispatch
//! scopes that contain no `.await`, so no `&VaultServiceImpl` is ever held
//! across an await point — this keeps `serve()`'s future `Send` (required for
//! `tokio::spawn`) without needing `VaultServiceImpl: Sync` (rusqlite's
//! `Connection` is `Send` but not `Sync`).
//!
//! [draft-ietf-miller-ssh-agent]: https://datatracker.ietf.org/doc/draft-miller-ssh-agent/

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use uuid::Uuid;

use crate::agent::identity::{load_ssh_identities, IdentityFilter, LoadedIdentity};
use crate::agent::signer::{
    EcdsaSigner, Ed25519Signer, RsaSigner, SignFlags, SignerError, SshAlgo, SshSigner,
};
use crate::commands::types::FieldSelector;
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::VaultServiceImpl;
use crate::types::audit::AuditOperation;

// ── wire protocol constants ─────────────────────────────────────────────────

/// `SSH_AGENT_FAILURE` — generic failure / unknown key / unsupported request.
const SSH_AGENT_FAILURE: u8 = 5;
/// `SSH_AGENT_SUCCESS` — success for messages that carry no data payload.
#[allow(dead_code)]
const SSH_AGENT_SUCCESS: u8 = 6;
/// `SSH_AGENTC_REQUEST_IDENTITIES` — client asks for the identity list.
const SSH_AGENTC_REQUEST_IDENTITIES: u8 = 11;
/// `SSH_AGENT_IDENTITIES_ANSWER` — server reply with the identity list.
const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
/// `SSH_AGENTC_SIGN_REQUEST` — client asks for a signature.
const SSH_AGENTC_SIGN_REQUEST: u8 = 13;
/// `SSH_AGENT_SIGN_RESPONSE` — server reply with the signature blob.
const SSH_AGENT_SIGN_RESPONSE: u8 = 14;

/// Sign-request flag bit requesting `rsa-sha2-256` (RFC 8332). RSA-only.
const SSH_AGENT_RSA_SHA2_256: u32 = 0x02;
/// Sign-request flag bit requesting `rsa-sha2-512` (RFC 8332). RSA-only.
const SSH_AGENT_RSA_SHA2_512: u32 = 0x04;

/// Upper bound on a single frame's payload length (256 KiB). The protocol's
/// largest legitimate message here is an RSA key/signature, far below this; the
/// cap exists only to reject a malicious length prefix before allocating.
const MAX_FRAME_LEN: usize = 256 * 1024;

// ── errors ──────────────────────────────────────────────────────────────────

/// Errors surfaced by [`AgentServer::start`] / [`AgentServer::serve`].
///
/// Per-connection errors are NOT propagated here: a single bad client must not
/// tear down the server, so dispatch failures are logged and the connection is
/// dropped (the client observes EOF or an `SSH_AGENT_FAILURE`).
#[derive(Debug, thiserror::Error)]
pub enum AgentServerError {
    /// Loading SSH identities from the vault at startup failed.
    #[error("failed to load SSH identities from vault")]
    LoadIdentities {
        #[source]
        source: crate::agent::identity::IdentityError,
    },
    /// Binding or securing the agent socket failed.
    #[error("failed to bind agent socket at {path}")]
    Bind {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The accept loop failed terminally (e.g. the listener was closed).
    #[error("agent accept loop failed")]
    Accept {
        #[source]
        source: std::io::Error,
    },
}

// ── AgentServer ─────────────────────────────────────────────────────────────

/// An ssh-agent-compatible server backed by the vault's SSH records.
///
/// Holds only public data: the loaded identities (public blobs + names) and a
/// `blob -> (record_id, algo)` index for O(1) sign-request lookup. The vault
/// handle is retained because the private key is fetched **per sign request**
/// (zero-cache: no signer/private key is kept across requests).
pub struct AgentServer {
    /// Vault used to decrypt key material on demand during the sign path.
    vault: VaultServiceImpl,
    /// Identities advertised in `REQUEST_IDENTITIES` (public blobs + names).
    identities: Vec<LoadedIdentity>,
    /// `public_blob -> (record_id, algo)` index for sign-request dispatch.
    blob_index: HashMap<Vec<u8>, (Uuid, SshAlgo)>,
    /// Filesystem path of the agent Unix socket.
    socket_path: PathBuf,
}

impl AgentServer {
    /// Build the server: load SSH identities through `filter` and index them by
    /// public blob. Does NOT bind the socket — call [`serve`](Self::serve) to
    /// bind and run the accept loop.
    ///
    /// Only public data is retained (zero-cache). No signer or private key is
    /// constructed here.
    pub fn start(
        vault: VaultServiceImpl,
        filter: IdentityFilter,
        socket_path: impl Into<PathBuf>,
    ) -> Result<Self, AgentServerError> {
        let identities = load_ssh_identities(&vault, &filter)
            .map_err(|source| AgentServerError::LoadIdentities { source })?;
        let blob_index = identities
            .iter()
            .map(|i| (i.public_blob.clone(), (i.record_id, i.algo)))
            .collect();
        Ok(Self {
            vault,
            identities,
            blob_index,
            socket_path: socket_path.into(),
        })
    }

    /// Bind the Unix socket and run the accept loop until a fatal accept error
    /// occurs or the runtime drops the future.
    ///
    /// The socket is created with mode `0600` and its parent directory is
    /// ensured with mode `0700`. Each accepted connection is served to
    /// completion: the server loops reading frames from THAT connection until
    /// the client closes (clean EOF) or a read/write error, answering
    /// potentially many requests over one file descriptor (the real-ssh
    /// pattern — `ssh` issues `REQUEST_IDENTITIES` + `SIGN_REQUEST` over a
    /// single agent socket). Dispatch is fully synchronous with no `&vault`
    /// held across an await.
    pub async fn serve(self) -> Result<(), AgentServerError> {
        // Ensure the parent directory exists with restrictive permissions.
        if let Some(parent) = self.socket_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|source| AgentServerError::Bind {
                    path: parent.to_path_buf(),
                    source,
                })?;
                set_mode(parent, 0o700)?;
            }
        }

        // Remove a stale socket file from a previous run (best effort).
        let _ = std::fs::remove_file(&self.socket_path);

        let listener =
            UnixListener::bind(&self.socket_path).map_err(|source| AgentServerError::Bind {
                path: self.socket_path.clone(),
                source,
            })?;
        // Restrict the socket to owner-only access.
        set_mode(&self.socket_path, 0o600)?;

        let Self {
            vault,
            identities,
            blob_index,
            ..
        } = self;

        loop {
            let (mut stream, _peer) = match listener.accept().await {
                Ok(pair) => pair,
                Err(source) => {
                    tracing::warn!(error = %source, "agent accept failed");
                    return Err(AgentServerError::Accept { source });
                }
            };

            // Serve ALL requests on THIS connection until the client closes
            // (clean EOF) or a read/write error, then go back to `accept()` for
            // the next client. Real OpenSSH `ssh` issues REQUEST_IDENTITIES +
            // SIGN_REQUEST (often several sign requests) over a single agent
            // file descriptor, so dropping the stream after one reply would
            // break interop. A single bad/unknown request returns
            // `SSH_AGENT_FAILURE` from `dispatch` and the inner loop continues
            // — the connection is NOT torn down for one bad request.
            loop {
                // Read one frame. No vault borrow is live across this await.
                let request = match read_frame(&mut stream).await {
                    Ok(Some(req)) => req,
                    Ok(None) => break, // client closed the connection cleanly
                    Err(source) => {
                        tracing::warn!(error = %source, "agent connection read failed");
                        break;
                    }
                };

                // Synchronous dispatch — the ONLY scope that borrows `vault`,
                // and it contains no `.await`. NLL ends the borrows at the
                // statement's close, before the `write_frame` await below, so
                // no `&vault` is held across an await point — keeping
                // `serve`'s future `Send` without requiring
                // `VaultServiceImpl: Sync`.
                let response = dispatch(&vault, &identities, &blob_index, &request);

                // Write the reply. No vault borrow is live across this await.
                if let Err(source) = write_frame(&mut stream, &response).await {
                    tracing::warn!(error = %source, "agent connection write failed");
                    break;
                }
            }
        }
    }
}

/// Set the file mode bits of `path` to `mode`.
fn set_mode(path: &Path, mode: u32) -> Result<(), AgentServerError> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|source| {
        AgentServerError::Bind {
            path: path.to_path_buf(),
            source,
        }
    })
}

// ── frame codec ─────────────────────────────────────────────────────────────

/// Read one length-prefixed frame, returning `None` on a clean EOF before any
/// bytes are read.
async fn read_frame(stream: &mut UnixStream) -> std::io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_LEN {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("agent frame length {len} exceeds cap {MAX_FRAME_LEN}"),
        ));
    }
    if len == 0 {
        return Ok(Some(Vec::new()));
    }
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;
    Ok(Some(payload))
}

/// Write one length-prefixed frame.
async fn write_frame(stream: &mut UnixStream, payload: &[u8]) -> std::io::Result<()> {
    stream
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await?;
    stream.write_all(payload).await?;
    stream.flush().await
}

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
    let len = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    if input.len() < 4 + len {
        return None;
    }
    Some((&input[4..4 + len], &input[4 + len..]))
}

// ── dispatch ────────────────────────────────────────────────────────────────

/// Pure synchronous dispatch: map one request payload to one response payload.
///
/// On any unsupported/unknown request, unknown key blob, or sign-path failure,
/// the reply is `SSH_AGENT_FAILURE` (never a panic, never propagated).
fn dispatch(
    vault: &VaultServiceImpl,
    identities: &[LoadedIdentity],
    blob_index: &HashMap<Vec<u8>, (Uuid, SshAlgo)>,
    request: &[u8],
) -> Vec<u8> {
    let Some((&msg_type, body)) = request.split_first() else {
        return failure();
    };
    match msg_type {
        SSH_AGENTC_REQUEST_IDENTITIES => answer_identities(identities),
        SSH_AGENTC_SIGN_REQUEST => handle_sign(vault, identities, blob_index, body),
        _ => failure(),
    }
}

/// Build the `SSH_AGENT_IDENTITIES_ANSWER` payload.
fn answer_identities(identities: &[LoadedIdentity]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.push(SSH_AGENT_IDENTITIES_ANSWER);
    out.extend_from_slice(&(identities.len() as u32).to_be_bytes());
    for id in identities {
        write_string(&mut out, &id.public_blob); // string key_blob
        write_string(&mut out, id.name.as_bytes()); // string comment
    }
    out
}

/// Handle a `SIGN_REQUEST` body: `<string key_blob><string data><u32 flags>`.
///
/// Looks up the blob in the index, decrypts the key material, builds a
/// temporary signer, signs, and returns `SSH_AGENT_SIGN_RESPONSE`. Any failure
/// (unknown blob, unsupported algo, decrypt error, sign error) yields
/// `SSH_AGENT_FAILURE`. The signer is dropped (zeroized) before returning.
///
/// After a successful sign, writes one `AuditOperation::SshSign` audit row
/// (record id + name + resolved wire algorithm). The audit write is
/// **best-effort**: on failure it is logged with `tracing::warn!` and the
/// successful sign response is still returned — an audit failure must never
/// block a successful signature seen by the SSH client.
fn handle_sign(
    vault: &VaultServiceImpl,
    identities: &[LoadedIdentity],
    blob_index: &HashMap<Vec<u8>, (Uuid, SshAlgo)>,
    body: &[u8],
) -> Vec<u8> {
    let Some((key_blob, rest)) = read_string(body) else {
        return failure();
    };
    let Some((data, rest)) = read_string(rest) else {
        return failure();
    };
    if rest.len() < 4 {
        return failure();
    }
    let flags = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]);

    let Some((record_id, algo)) = blob_index.get(key_blob) else {
        // Unknown / unauthorized key blob.
        return failure();
    };
    let record_id = *record_id;
    let algo = *algo;

    // Decode the wire flags once into SignFlags; `sign` consumes them and they
    // are reused to label the SshSign audit detail with the exact algorithm
    // variant (e.g. rsa-sha2-256 vs rsa-sha2-512).
    let sign_flags = SignFlags {
        rsa_sha2_256: flags & SSH_AGENT_RSA_SHA2_256 != 0,
        rsa_sha2_512: flags & SSH_AGENT_RSA_SHA2_512 != 0,
    };

    let sig_blob = match sign(vault, record_id, data, sign_flags, algo) {
        Ok(sig) => sig,
        Err(err) => {
            tracing::warn!(error = %err, "agent sign path failed");
            return failure();
        }
    };

    // BEST-EFFORT audit of the successful sign. The sign response below is
    // built from `sig_blob` unconditionally; this audit write cannot influence
    // the returned signature.
    let name = identities
        .iter()
        .find(|i| i.record_id == record_id)
        .map(|i| i.name.as_str());
    if let Err(err) = vault._write_audit(
        AuditOperation::SshSign,
        Some(record_id),
        name,
        Some(algo.wire_name(sign_flags)),
    ) {
        tracing::warn!(
            error = %err,
            record_id = %record_id,
            "ssh sign audit write failed; signature still returned"
        );
    }

    // `SSH_AGENT_SIGN_RESPONSE` = byte 14 + `string <sig blob>`.
    let mut out = Vec::with_capacity(1 + 4 + sig_blob.len());
    out.push(SSH_AGENT_SIGN_RESPONSE);
    write_string(&mut out, &sig_blob);
    out
}

/// Decrypt key material for `record_id`, build a temporary signer for `algo`,
/// sign `data` with `flags`, drop the signer, and return the raw SSH wire-format
/// signature blob.
///
/// `algo` selects the signer: `SshAlgo::Ed25519` → [`Ed25519Signer`],
/// `SshAlgo::Rsa` → [`RsaSigner`], `SshAlgo::Ecdsa(_)` → [`EcdsaSigner`].
/// Adding a new algorithm is a local change: extend the match with another arm.
///
/// `flags` are the signer-layer's algorithm-agnostic [`SignFlags`] (RSA SHA-2
/// variant selection per RFC 8332; ed25519 ignores both). The wire constants
/// `SSH_AGENT_RSA_SHA2_256` (0x02) and `SSH_AGENT_RSA_SHA2_512` (0x04) are
/// decoded by the caller ([`handle_sign`]) so this function and the signer
/// layer stay wire-protocol-agnostic.
///
/// # Audit
///
/// Key material is decrypted via [`VaultServiceImpl::decrypt_field_no_audit`]:
/// the sign path owns its own audit and writes one `AuditOperation::SshSign`
/// row from [`handle_sign`] after a successful sign. Decrypting through the
/// audited `decrypt_field` would add a misleading `RecordViewPassword` row
/// (the user never "viewed" the private key; the agent used it internally).
fn sign(
    vault: &VaultServiceImpl,
    record_id: Uuid,
    data: &[u8],
    flags: SignFlags,
    algo: SshAlgo,
) -> Result<Vec<u8>, SignError> {
    // Fetch the private key PEM WITHOUT a RecordViewPassword audit row. The
    // caller writes a single SshSign audit entry covering this sign.
    // FieldSelector::Password maps to the SSH `private_key` field (see
    // services::vault::record::helpers).
    let pem = vault
        .decrypt_field_no_audit(record_id, FieldSelector::Password)
        .map_err(SignError::DecryptPrivateKey)?;

    // Fetch the passphrase, if any. FieldSelector::Passphrase returns
    // InvalidField when the stored passphrase is None; treat both that and an
    // empty string as "no passphrase" (None).
    let passphrase = vault
        .decrypt_field_no_audit(record_id, FieldSelector::Passphrase)
        .ok()
        .map(|s| s.expose().to_string())
        .filter(|s| !s.is_empty());

    // Build a temporary signer, sign, and drop immediately (zero-cache). The
    // ed25519 signer's seed is Zeroizing; the RSA signer's private key is
    // ZeroizeOnDrop — both zeroize on drop.
    let sig = {
        match algo {
            SshAlgo::Ed25519 => {
                let signer = Ed25519Signer::from_openssh(pem.expose(), passphrase.as_deref())
                    .map_err(SignError::BuildSigner)?;
                signer.sign(data, flags).map_err(SignError::Sign)?
                // `signer` dropped here: seed zeroized.
            }
            SshAlgo::Rsa => {
                let signer = RsaSigner::from_openssh(pem.expose(), passphrase.as_deref())
                    .map_err(SignError::BuildSigner)?;
                signer.sign(data, flags).map_err(SignError::Sign)?
                // `signer` dropped here: RsaPrivateKey zeroized on drop.
            }
            SshAlgo::Ecdsa(_) => {
                let signer = EcdsaSigner::from_openssh(pem.expose(), passphrase.as_deref())
                    .map_err(SignError::BuildSigner)?;
                signer.sign(data, flags).map_err(SignError::Sign)?
                // `signer` dropped here: the curve `ecdsa::SigningKey`
                // (p256/p384/p521) is zeroized on drop via its own
                // `ZeroizeOnDrop` impl.
            } // `SshAlgo` is exhaustive above; adding a new variant without a
              // signer arm is a compile error (fail-loud at compile time) rather
              // than a silent runtime fallback.
        }
    };

    Ok(sig)
}

/// Sanitized aggregation of sign-path failures for logging. The inner sources
/// carry no private key material in their Display.
#[derive(Debug, thiserror::Error)]
enum SignError {
    #[error("decrypting the private key failed")]
    DecryptPrivateKey(#[source] VaultError),
    #[error("building the signer failed")]
    BuildSigner(#[source] SignerError),
    #[error("signing failed")]
    Sign(#[source] SignerError),
}

/// A minimal `SSH_AGENT_FAILURE` payload (single byte).
fn failure() -> Vec<u8> {
    vec![SSH_AGENT_FAILURE]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_string_prepends_be_length() {
        let mut out = Vec::new();
        write_string(&mut out, b"abc");
        assert_eq!(out, vec![0, 0, 0, 3, b'a', b'b', b'c']);
    }

    #[test]
    fn read_string_round_trips() {
        let mut buf = Vec::new();
        write_string(&mut buf, b"hello");
        write_string(&mut buf, b"world");
        let (a, rest) = read_string(&buf).expect("first string");
        assert_eq!(a, b"hello");
        let (b, rest) = read_string(rest).expect("second string");
        assert_eq!(b, b"world");
        assert!(rest.is_empty());
    }

    #[test]
    fn read_string_rejects_truncated_input() {
        assert!(read_string(&[0, 0, 0]).is_none());
        assert!(read_string(&[0, 0, 0, 5, b'a']).is_none());
    }

    #[test]
    fn failure_payload_is_single_byte() {
        assert_eq!(failure(), vec![SSH_AGENT_FAILURE]);
    }

    #[test]
    fn answer_identities_encodes_count_and_blobs() {
        let identities = vec![LoadedIdentity {
            record_id: Uuid::nil(),
            name: "k".to_string(),
            algo: SshAlgo::Ed25519,
            public_blob: vec![0xAA; 4],
        }];
        let out = answer_identities(&identities);
        assert_eq!(out[0], SSH_AGENT_IDENTITIES_ANSWER);
        assert_eq!(&out[1..5], &[0, 0, 0, 1]); // count = 1
        let (blob, rest) = read_string(&out[5..]).unwrap();
        assert_eq!(blob, &[0xAA; 4]);
        let (comment, tail) = read_string(rest).unwrap();
        assert_eq!(comment, b"k");
        assert!(tail.is_empty());
    }

    #[test]
    fn dispatch_unknown_message_type_yields_failure() {
        let vault = vault_fixture();
        let resp = dispatch(
            &vault,
            &[],
            &HashMap::new(),
            &[99u8], // unknown message type
        );
        assert_eq!(resp, vec![SSH_AGENT_FAILURE]);
    }

    #[test]
    fn dispatch_empty_payload_yields_failure() {
        let vault = vault_fixture();
        let resp = dispatch(&vault, &[], &HashMap::new(), &[]);
        assert_eq!(resp, vec![SSH_AGENT_FAILURE]);
    }

    #[test]
    fn handle_sign_unknown_blob_yields_failure() {
        let vault = vault_fixture();
        let index = HashMap::new();
        let mut body = Vec::new();
        write_string(&mut body, b"not-in-index");
        write_string(&mut body, b"data");
        body.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(
            handle_sign(&vault, &[], &index, &body),
            vec![SSH_AGENT_FAILURE]
        );
    }

    #[test]
    fn handle_sign_truncated_body_yields_failure() {
        let vault = vault_fixture();
        let index = HashMap::new();
        // Only the key_blob string, no data/flags.
        let mut body = Vec::new();
        write_string(&mut body, b"blob");
        assert_eq!(
            handle_sign(&vault, &[], &index, &body),
            vec![SSH_AGENT_FAILURE]
        );
    }

    // =========================================================================
    // Audit: a successful SIGN_REQUEST writes exactly one SshSign audit row with
    // the record id, name, and resolved algorithm — and does NOT write a
    // RecordViewPassword row (the sign path uses a no-audit decrypt, because the
    // user never "viewed" a password; the agent used the private key internally).
    // =========================================================================

    /// A real unencrypted ed25519 OpenSSH private key (matches the integration
    /// test fixture). Stored as the SSH record's `private_key`.
    const AUDIT_ED25519_PEM: &str = include_str!("../../tests/fixtures/test_ed25519");

    /// OpenSSH public key string for `AUDIT_ED25519_PEM`, stored as the record's
    /// `public_key` so identity loading can parse a wire-format blob.
    const AUDIT_ED25519_PUB_SSH: &str = "ssh-ed25519 \
         AAAAC3NzaC1lZDI1NTE5AAAAIPddLwxmYUz+k43Vr+cahIy1iOROowugaJr8lQ6Tmi2V \
         test-ed25519@oak-keyring";

    /// Build an unlocked in-memory vault holding one ed25519 SSH record, returning
    /// `(vault, record_id)`. The record's name is `"github-key"`.
    fn vault_with_ed25519_record() -> (VaultServiceImpl, Uuid) {
        use crate::crypto::bip39::{MnemonicLanguage, Passkey};
        use crate::db::schema::init_db_in_memory;
        use crate::types::credential::{CredentialType, EncryptedPayload};
        use crate::types::record::CreateRecordParams;
        use crate::types::sensitive::SecureStr;
        let conn = init_db_in_memory().expect("in-memory db");
        let mut svc = VaultServiceImpl::new(conn);
        let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
        svc.unlock_with_mnemonic(&mnemonic)
            .expect("unlock must succeed");
        let id = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Ssh,
                payload: EncryptedPayload::Ssh {
                    name: "github-key".to_string(),
                    public_key: AUDIT_ED25519_PUB_SSH.to_string(),
                    private_key: Some(SecureStr::new(AUDIT_ED25519_PEM.to_string())),
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

    /// Count audit entries of `operation` in `vault`.
    fn count_audit(
        vault: &VaultServiceImpl,
        operation: crate::types::audit::AuditOperation,
    ) -> usize {
        use crate::commands::types::AuditFilter;
        let filter = AuditFilter {
            operation: Some(operation),
            ..Default::default()
        };
        vault.query_audit_log(&filter).expect("audit query").1
    }

    #[test]
    fn sign_request_writes_exactly_one_ssh_sign_audit_row() {
        use crate::agent::identity::{load_ssh_identities, IdentityFilter};
        use crate::commands::types::AuditFilter;
        use crate::types::audit::AuditOperation;

        let (vault, record_id) = vault_with_ed25519_record();

        // Load identities + build the blob index exactly as AgentServer::start does.
        let identities =
            load_ssh_identities(&vault, &IdentityFilter::default()).expect("load identities");
        assert_eq!(identities.len(), 1, "fixture vault has one SSH record");
        let blob_index: HashMap<Vec<u8>, (Uuid, SshAlgo)> = identities
            .iter()
            .map(|i| (i.public_blob.clone(), (i.record_id, i.algo)))
            .collect();
        let blob = identities[0].public_blob.clone();

        // Build a SIGN_REQUEST wire body: <string key_blob><string data><u32 flags>.
        let mut request = vec![SSH_AGENTC_SIGN_REQUEST];
        write_string(&mut request, &blob);
        write_string(&mut request, b"authenticate me, agent");
        request.extend_from_slice(&0u32.to_be_bytes()); // flags = 0

        // Before the sign: no SshSign, no RecordViewPassword.
        assert_eq!(count_audit(&vault, AuditOperation::SshSign), 0);
        assert_eq!(count_audit(&vault, AuditOperation::RecordViewPassword), 0);

        let response = dispatch(&vault, &identities, &blob_index, &request);
        assert_eq!(
            response[0], SSH_AGENT_SIGN_RESPONSE,
            "sign must succeed before auditing the audit row"
        );

        // After a successful sign: EXACTLY one SshSign row.
        assert_eq!(
            count_audit(&vault, AuditOperation::SshSign),
            1,
            "a successful sign must write exactly one SshSign audit row"
        );

        // And NO RecordViewPassword: the sign path decrypts the private key via
        // a no-audit decrypt, because the agent using the key internally is not
        // a user "view password" event. SshSign is the single, accurate event.
        assert_eq!(
            count_audit(&vault, AuditOperation::RecordViewPassword),
            0,
            "sign path must not double-audit with RecordViewPassword"
        );

        // Inspect the SshSign row's fields.
        let filter = AuditFilter {
            operation: Some(AuditOperation::SshSign),
            ..Default::default()
        };
        let (entries, _total) = vault.query_audit_log(&filter).expect("query ssh sign");
        assert_eq!(entries.len(), 1);
        let row = &entries[0];
        assert_eq!(row.operation, AuditOperation::SshSign);
        assert_eq!(row.record_id, Some(record_id));
        assert_eq!(row.record_name.as_deref(), Some("github-key"));
        assert_eq!(
            row.detail.as_deref(),
            Some("ssh-ed25519"),
            "detail must be the resolved wire algorithm name"
        );
    }

    /// Build an unlocked in-memory vault for dispatch-level unit tests (no SSH
    /// records needed — these tests exercise parsing/failure paths only).
    fn vault_fixture() -> VaultServiceImpl {
        use crate::crypto::bip39::{MnemonicLanguage, Passkey};
        use crate::db::schema::init_db_in_memory;
        let conn = init_db_in_memory().expect("in-memory db");
        let mut svc = VaultServiceImpl::new(conn);
        let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
        svc.unlock_with_mnemonic(&mnemonic)
            .expect("unlock must succeed");
        svc
    }
}
