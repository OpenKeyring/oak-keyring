//! SSH identity loading for the agent's `REQUEST_IDENTITIES` response.
//!
//! [`load_ssh_identities`] enumerates the vault's SSH records and returns one
//! [`LoadedIdentity`] per matching record. This module deliberately implements
//! the spec's **zero-cache** contract: a `LoadedIdentity` carries only the
//! `record_id`, the vault record `name`, the resolved [`SshAlgo`], and the
//! SSH wire-format `public_blob`. It does **not** RETAIN or EXPOSE a signer
//! or a private key — the private key is fetched per-sign in the server task
//! (Task 5) by looking the record up via `record_id`.
//!
//! # Zero-cache scope (RETENTION, not transient decrypt)
//!
//! The vault encrypts the whole `EncryptedPayload` as one AEAD blob, so any
//! field decrypt (name or public key) transiently materializes the full
//! payload plaintext — including the private key — in memory; only the PARSE
//! is field-scoped. This is inherent to the vault's AEAD design, not an agent
//! concern. The agent's zero-cache guarantee is about RETENTION (nothing kept
//! in `LoadedIdentity`), not about avoiding transient decrypt.
//!
//! # Vault field mapping
//!
//! The vault exposes the SSH public key through [`FieldSelector::Username`]
//! (the service's historical alias for the SSH `public_key` string — there is
//! no dedicated `PublicKey` selector variant). The OpenSSH string is parsed
//! with [`ssh_key::PublicKey::from_openssh`] to obtain both the wire blob
//! ([`PublicKey::to_bytes`]) and the algorithm ([`PublicKey::algorithm`]).
//!
//! [`FieldSelector::Username`]: crate::commands::types::FieldSelector::Username
//! [`PublicKey::to_bytes`]: ssh_key::PublicKey::to_bytes
//! [`PublicKey::algorithm`]: ssh_key::PublicKey::algorithm

use regex::Regex;
use ssh_key::{Algorithm, PublicKey};
use thiserror::Error;
use uuid::Uuid;

use crate::agent::signer::{EcdsaCurve, SshAlgo};
use crate::commands::types::FieldSelector;
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::VaultServiceImpl;
use crate::types::credential::CredentialType;

/// A vault SSH record projected into the data the SSH agent needs to answer
/// `SSH_AGENTC_REQUEST_IDENTITIES`.
///
/// **Zero-cache (RETENTION):** this struct intentionally holds NO signer and
/// does NOT RETAIN or EXPOSE any private key material — only the public blob
/// and enough metadata (`record_id`, `name`, `algo`) for the server to locate
/// and sign with the key on demand. Note: building a `LoadedIdentity` does
/// transiently decrypt the vault payload (see module docs); the zero-cache
/// guarantee is that nothing is KEPT, not that the private key is never
/// touched in memory.
#[derive(Debug, Clone)]
pub struct LoadedIdentity {
    /// Vault record id; the server uses this to fetch the private key per-sign.
    pub record_id: Uuid,
    /// Vault record name (used for display / logging, not for SSH wire format).
    pub name: String,
    /// Resolved SSH algorithm family.
    pub algo: SshAlgo,
    /// SSH wire-format public key blob (the `string` carried per identity in
    /// `SSH_AGENTC_REQUEST_IDENTITIES`).
    pub public_blob: Vec<u8>,
}

/// Whitelist filter applied to the vault record `name` when loading identities.
///
/// Empty `only` + `None` `allow` matches everything. Otherwise a name is
/// accepted when it is present in `only` **or** matches the `allow` regex.
/// This lets the agent expose a precise subset of stored keys.
#[derive(Debug, Default, Clone)]
pub struct IdentityFilter {
    /// Exact record names to include. Empty means "no exact-name restriction".
    pub only: Vec<String>,
    /// Optional regex; a matching name is included. `None` means "no regex".
    pub allow: Option<Regex>,
}

impl IdentityFilter {
    /// Returns `true` when `name` should be included under this filter.
    ///
    /// Empty `only` + `None` `allow` is the match-all default. With any
    /// constraint set, a name is accepted if it is in `only` OR matches `allow`.
    pub fn matches(&self, name: &str) -> bool {
        if self.only.is_empty() && self.allow.is_none() {
            return true;
        }
        let in_only = self.only.iter().any(|n| n == name);
        let matches_allow = self.allow.as_ref().is_some_and(|re| re.is_match(name));
        in_only || matches_allow
    }
}

/// Errors surfaced while loading SSH identities. Display messages never carry
/// private key material; underlying sources are attached for diagnostics only.
#[derive(Debug, Error)]
pub enum IdentityError {
    /// Vault enumeration or field decryption failed.
    #[error("vault access failed while loading SSH identities")]
    Vault {
        #[source]
        source: VaultError,
    },
    /// The stored SSH public key string could not be parsed as OpenSSH.
    #[error("failed to parse stored SSH public key as OpenSSH")]
    ParsePublicKey {
        #[source]
        source: ssh_key::Error,
    },
    /// Encoding the SSH public key to its wire format failed.
    #[error("failed to encode SSH public key to wire format")]
    EncodePublicKey {
        #[source]
        source: ssh_key::Error,
    },
    /// The key algorithm is recognized but not one the agent exposes
    /// (e.g. DSA, FIDO/U2F security-key variants, or unknown algorithms).
    #[error("unsupported SSH key algorithm for agent identity: {0}")]
    UnsupportedAlgorithm(String),
}

/// Load every SSH record in `vault` whose name passes `filter`, returning one
/// [`LoadedIdentity`] per record.
///
/// Only [`CredentialType::Ssh`] records are considered; all other credential
/// types are skipped. Only the record name and the public key string are
/// PARSED and RETAINED — honoring the zero-cache contract (nothing beyond
/// those fields is kept). Note: because the vault uses whole-payload AEAD,
/// each field decrypt transiently materializes the full payload plaintext in
/// memory; the private key is not retained, only not extracted.
pub fn load_ssh_identities(
    vault: &VaultServiceImpl,
    filter: &IdentityFilter,
) -> Result<Vec<LoadedIdentity>, IdentityError> {
    let records = vault
        .list_all_stored_records()
        .map_err(|source| IdentityError::Vault { source })?;

    let mut identities = Vec::new();
    for record in records {
        if record.credential_type != CredentialType::Ssh {
            continue;
        }

        // Decrypt to obtain the name for filtering. The PARSE is name-scoped,
        // but because the vault uses whole-payload AEAD the decrypt transiently
        // materializes the full payload plaintext in memory (private key
        // included); only the name is extracted and retained.
        let name = vault
            .decrypt_record_name_for_sync(&record)
            .map_err(|source| IdentityError::Vault { source })?;
        if !filter.matches(&name) {
            continue;
        }

        // Read the OpenSSH public key string. For SSH records the service maps
        // `FieldSelector::Username` to the stored `public_key` field.
        let public_ssh = vault
            .decrypt_field(record.id, FieldSelector::Username)
            .map_err(|source| IdentityError::Vault { source })?;

        let (algo, public_blob) = parse_public_key(public_ssh.expose())?;

        identities.push(LoadedIdentity {
            record_id: record.id,
            name,
            algo,
            public_blob,
        });
    }

    Ok(identities)
}

/// Parse an OpenSSH public key string into `(SshAlgo, wire_blob)`.
///
/// `wire_blob` is the SSH wire-format public key (`PublicKey::to_bytes`), i.e.
/// the exact bytes the agent protocol carries per identity. Ed25519, RSA, and
/// ECDSA (P-256/P-384) are recognized; any other algorithm is a loud error so
/// the agent never advertises a key it cannot sign.
fn parse_public_key(openssh_str: &str) -> Result<(SshAlgo, Vec<u8>), IdentityError> {
    let public = PublicKey::from_openssh(openssh_str)
        .map_err(|source| IdentityError::ParsePublicKey { source })?;

    let algo = map_algorithm(&public.algorithm())?;
    let public_blob = public
        .to_bytes()
        .map_err(|source| IdentityError::EncodePublicKey { source })?;

    Ok((algo, public_blob))
}

/// Map an [`ssh_key::Algorithm`] to the agent's [`SshAlgo`].
///
/// Ed25519/RSA/ECDSA-P256/P384 are accepted (signers land across Tasks 3/7/8;
/// loading recognizes all of them so the identity list is complete). DSA,
/// P-521, and FIDO/U2F security-key variants are rejected loudly — the agent
/// cannot sign with them, so it must not advertise them.
fn map_algorithm(algo: &Algorithm) -> Result<SshAlgo, IdentityError> {
    match algo {
        Algorithm::Ed25519 => Ok(SshAlgo::Ed25519),
        Algorithm::Rsa { .. } => Ok(SshAlgo::Rsa),
        Algorithm::Ecdsa { curve } => match curve {
            ssh_key::EcdsaCurve::NistP256 => Ok(SshAlgo::Ecdsa(EcdsaCurve::P256)),
            ssh_key::EcdsaCurve::NistP384 => Ok(SshAlgo::Ecdsa(EcdsaCurve::P384)),
            ssh_key::EcdsaCurve::NistP521 => Err(IdentityError::UnsupportedAlgorithm(
                algo.as_str().to_string(),
            )),
        },
        // DSA, FIDO/U2F (Sk*), and unknown algorithms are not signable here.
        other => Err(IdentityError::UnsupportedAlgorithm(
            other.as_str().to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_default_matches_all() {
        let f = IdentityFilter::default();
        assert!(f.matches("anything"));
        assert!(f.matches(""));
    }

    #[test]
    fn filter_only_matches_membership() {
        let f = IdentityFilter {
            only: vec!["a".to_string(), "b".to_string()],
            allow: None,
        };
        assert!(f.matches("a"));
        assert!(f.matches("b"));
        assert!(!f.matches("c"));
    }

    #[test]
    fn filter_allow_matches_regex() {
        let f = IdentityFilter {
            only: vec![],
            allow: Some(Regex::new("^key-").unwrap()),
        };
        assert!(f.matches("key-1"));
        assert!(!f.matches("other"));
    }

    #[test]
    fn filter_only_or_allow_is_union() {
        let f = IdentityFilter {
            only: vec!["exact".to_string()],
            allow: Some(Regex::new("^re-").unwrap()),
        };
        assert!(f.matches("exact"));
        assert!(f.matches("re-1"));
        assert!(!f.matches("nope"));
    }

    #[test]
    fn parse_public_key_ed25519_yields_ed25519_algo_and_blob() {
        let s =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPddLwxmYUz+k43Vr+cahIy1iOROowugaJr8lQ6Tmi2V k";
        let (algo, blob) = parse_public_key(s).expect("ed25519 parses");
        assert_eq!(algo, SshAlgo::Ed25519);
        // Wire blob starts with the algorithm-name string length prefix (11).
        assert_eq!(&blob[0..4], [0, 0, 0, 11]);
        assert_eq!(&blob[4..15], b"ssh-ed25519");
    }

    #[test]
    fn parse_public_key_rejects_malformed_input() {
        let result = parse_public_key("not a key");
        assert!(result.is_err(), "malformed input must fail loudly");
    }
}
