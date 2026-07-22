//! SSH agent signer abstraction.
//!
//! Produces raw SSH wire-format signature blobs suitable for placing directly
//! into an `SSH_AGENT_SIGN_RESPONSE`. This is deliberately NOT
//! [`ssh_key::SshSig`] (the `ssh-keygen -Y sign` / SSHSIG format): the SSH
//! agent protocol exchanges algorithm-specific raw signatures, wrapped only as
//! `string <algorithm_name>` + `string <signature_bytes>`.
//!
//! Only Ed25519 is implemented in this task; RSA / ECDSA signers are deferred
//! to later tasks (the [`SshAlgo`] enum keeps their slots in the public API).

use ed25519_dalek::{Signature, SigningKey};
use thiserror::Error;
use zeroize::Zeroizing;

/// Result alias for all signer operations.
pub type SignerResult<T> = std::result::Result<T, SignerError>;

/// SSH algorithm families the agent backend can sign with.
///
/// RSA and ECDSA variants are declared for API completeness; only Ed25519 is
/// constructible today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshAlgo {
    /// `ssh-ed25519`
    Ed25519,
    /// `ssh-rsa` (not yet implemented)
    Rsa,
    /// `ecdsa-sha2-nistp256` / `nistp384` (not yet implemented)
    Ecdsa(EcdsaCurve),
}

/// Named ECDSA curves for [`SshAlgo::Ecdsa`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcdsaCurve {
    /// NIST P-256 (`nistp256`)
    P256,
    /// NIST P-384 (`nistp384`)
    P384,
}

/// Per-sign request flags.
///
/// Only meaningful for RSA (selecting the SHA-2 signature variant). Ed25519
/// ignores both fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SignFlags {
    /// Request `rsa-sha2-256` signatures (RFC 8332).
    pub rsa_sha2_256: bool,
    /// Request `rsa-sha2-512` signatures (RFC 8332).
    pub rsa_sha2_512: bool,
}

/// Sanitized error type for the signer layer.
///
/// Display messages never carry private key material; the underlying
/// [`ssh_key::Error`] is attached only as a non-displayed source for
/// diagnostics.
#[derive(Debug, Error)]
pub enum SignerError {
    /// OpenSSH PEM parsing failed.
    #[error("failed to parse OpenSSH private key")]
    ParseKey {
        #[source]
        source: ssh_key::Error,
    },
    /// Decryption of a passphrase-protected key failed (wrong/missing passphrase).
    #[error("failed to decrypt passphrase-protected private key")]
    Decrypt {
        #[source]
        source: ssh_key::Error,
    },
    /// Encoding the SSH public key failed.
    #[error("failed to encode SSH public key")]
    PublicKey {
        #[source]
        source: ssh_key::Error,
    },
    /// The parsed key is not an Ed25519 key.
    #[error("unsupported SSH key type: expected ed25519")]
    UnsupportedKeyType,
    /// A passphrase was supplied for a key that is not encrypted.
    #[error("passphrase provided but the key is not encrypted")]
    UnexpectedPassphrase,
    /// An encrypted key was supplied without a passphrase.
    #[error("key is encrypted but no passphrase was provided")]
    MissingPassphrase,
}

/// Agent-facing capability for producing SSH signatures.
///
/// Implementations must be [`Send`] + [`Sync`] so they can live behind an
/// `Arc<dyn SshSigner>` shared by the agent's connection handler.
#[cfg_attr(test, mockall::automock)]
pub trait SshSigner: Send + Sync {
    /// Algorithm this signer produces signatures for.
    fn algorithm(&self) -> SshAlgo;

    /// OpenSSH-format public key string (e.g. `ssh-ed25519 AAAA... comment`),
    /// as used in `SSH_AGENTC_REQUEST_IDENTITIES` / `authorized_keys`.
    fn public_key_ssh(&self) -> SignerResult<String>;

    /// Sign `data`, returning a raw SSH wire-format signature blob ready for
    /// `SSH_AGENT_SIGN_RESPONSE`: `string <algorithm>` + `string <sig bytes>`.
    ///
    /// `flags` select RSA SHA-2 variants and are ignored by Ed25519.
    fn sign(&self, data: &[u8], flags: SignFlags) -> SignerResult<Vec<u8>>;
}

/// Ed25519 SSH signer.
///
/// Holds the 32-byte secret seed, zeroized on drop. The OpenSSH public key
/// string is precomputed at construction for cheap identity listing.
pub struct Ed25519Signer {
    /// 32-byte Ed25519 secret seed. Wrapped in [`Zeroizing`] so the bytes are
    /// zeroed when this field is dropped.
    seed: Zeroizing<[u8; 32]>,
    /// Precomputed OpenSSH public key string.
    public_ssh: String,
}

impl Ed25519Signer {
    /// Build a signer from an OpenSSH PEM private key.
    ///
    /// `passphrase` is required when (and only when) the key is
    /// passphrase-protected; supplying a passphrase for an unencrypted key, or
    /// omitting it for an encrypted key, is a loud error.
    pub fn from_openssh(pem: &str, passphrase: Option<&str>) -> SignerResult<Self> {
        let parsed = ssh_key::PrivateKey::from_openssh(pem)
            .map_err(|source| SignerError::ParseKey { source })?;

        // Resolve encryption state against the supplied passphrase.
        let key = if parsed.is_encrypted() {
            let passphrase = passphrase.ok_or(SignerError::MissingPassphrase)?;
            parsed
                .decrypt(passphrase)
                .map_err(|source| SignerError::Decrypt { source })?
        } else {
            match passphrase {
                Some(_) => return Err(SignerError::UnexpectedPassphrase),
                None => parsed,
            }
        };

        let keypair = match key.key_data() {
            ssh_key::private::KeypairData::Ed25519(kp) => kp,
            _ => return Err(SignerError::UnsupportedKeyType),
        };

        // `Ed25519PrivateKey: AsRef<[u8; 32]>` — the raw 32-byte seed.
        let seed = *keypair.private.as_ref();
        let public_ssh = key
            .public_key()
            .to_openssh()
            .map_err(|source| SignerError::PublicKey { source })?;

        Ok(Self {
            seed: Zeroizing::new(seed),
            public_ssh,
        })
    }

    /// Reconstruct the dalek signing key from the held seed on demand.
    ///
    /// Done per `sign()` rather than stored, because `ed25519_dalek::SigningKey`
    /// implements `ZeroizeOnDrop` but not `Zeroize`, so it cannot be wrapped
    /// in `Zeroizing` directly. The reconstruction cost (a SHA-512 expansion)
    /// is negligible compared to signing itself and SSH agent call frequency.
    fn signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.seed)
    }
}

impl SshSigner for Ed25519Signer {
    fn algorithm(&self) -> SshAlgo {
        SshAlgo::Ed25519
    }

    fn public_key_ssh(&self) -> SignerResult<String> {
        Ok(self.public_ssh.clone())
    }

    fn sign(&self, data: &[u8], _flags: SignFlags) -> SignerResult<Vec<u8>> {
        use ed25519_dalek::Signer as _;
        let signature: Signature = self.signing_key().sign(data);
        Ok(ed25519_wire_signature(&signature.to_bytes()))
    }
}

/// Build the SSH agent wire-format signature blob for Ed25519:
/// `string "ssh-ed25519"` + `string <64-byte signature>`, where each `string`
/// is a 4-byte big-endian length prefix followed by the bytes.
fn ed25519_wire_signature(sig: &[u8; ed25519_dalek::SIGNATURE_LENGTH]) -> Vec<u8> {
    const ALG: &[u8] = b"ssh-ed25519";
    let mut out = Vec::with_capacity(4 + ALG.len() + 4 + sig.len());
    out.extend_from_slice(&(ALG.len() as u32).to_be_bytes());
    out.extend_from_slice(ALG);
    out.extend_from_slice(&(sig.len() as u32).to_be_bytes());
    out.extend_from_slice(sig);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_signature_layout_is_correct() {
        let sig = [0xABu8; ed25519_dalek::SIGNATURE_LENGTH];
        let blob = ed25519_wire_signature(&sig);

        // string "ssh-ed25519"
        assert_eq!(&blob[0..4], [0, 0, 0, 11]);
        assert_eq!(&blob[4..15], b"ssh-ed25519");
        // string <64-byte sig>
        assert_eq!(&blob[15..19], [0, 0, 0, 64]);
        assert_eq!(&blob[19..83], &[0xAB; 64]);
        assert_eq!(blob.len(), 4 + 11 + 4 + 64);
    }

    #[test]
    fn default_flags_are_all_false() {
        let flags = SignFlags::default();
        assert!(!flags.rsa_sha2_256);
        assert!(!flags.rsa_sha2_512);
    }
}
