//! SSH agent signer abstraction.
//!
//! Produces raw SSH wire-format signature blobs suitable for placing directly
//! into an `SSH_AGENT_SIGN_RESPONSE`. This is deliberately NOT
//! [`ssh_key::SshSig`] (the `ssh-keygen -Y sign` / SSHSIG format): the SSH
//! agent protocol exchanges algorithm-specific raw signatures, wrapped only as
//! `string <algorithm_name>` + `string <signature_bytes>`.
//!
//! Ed25519 and RSA (PKCS#1 v1.5 over SHA-256 / SHA-512, RFC 8332) are
//! implemented. ECDSA signers are deferred to a later task (the [`SshAlgo`]
//! enum keeps its slot in the public API).

use ed25519_dalek::{Signature, SigningKey};
use sha2::{Digest, Sha256, Sha512};
use thiserror::Error;
use zeroize::Zeroizing;

/// Result alias for all signer operations.
pub type SignerResult<T> = std::result::Result<T, SignerError>;

/// SSH algorithm families the agent backend can sign with.
///
/// Ed25519 and RSA are constructible today; ECDSA is declared for API
/// completeness and resolved by the identity layer, but not yet signable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshAlgo {
    /// `ssh-ed25519`
    Ed25519,
    /// `ssh-rsa` / `rsa-sha2-256` / `rsa-sha2-512` (RFC 8332).
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
/// Only meaningful for RSA (selecting the SHA-2 signature variant, per
/// RFC 8332). Ed25519 ignores both fields.
///
/// # RSA precedence
///
/// - If `rsa_sha2_512` is set, the signature is `rsa-sha2-512` (SHA-512 wins
///   over SHA-256 — matches OpenSSH: the stronger hash is preferred when the
///   client offers both).
/// - Else if `rsa_sha2_256` is set, the signature is `rsa-sha2-256`.
/// - If NEITHER flag is set, the signature DEFAULTS to `rsa-sha2-256`. Modern
///   ssh refuses the legacy SHA-1 `ssh-rsa` variant, so returning SHA-1 here
///   would break interop. SHA-1 `ssh-rsa` support is a spec Non-Goal.
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
    /// Converting the parsed RSA keypair into a `rsa::RsaPrivateKey` failed
    /// (e.g. malformed CRT components, or a key below the 2048-bit minimum
    /// enforced by ssh-key 0.6.7). Display carries no key material.
    #[error("failed to convert SSH RSA keypair into an RSA private key")]
    RsaKey {
        #[source]
        source: ssh_key::Error,
    },
    /// The parsed key is not the expected algorithm (e.g. an RSA PEM supplied
    /// to `Ed25519Signer`, or an ed25519 PEM supplied to `RsaSigner`).
    #[error("unsupported SSH key type: expected {expected}")]
    UnsupportedKeyType { expected: &'static str },
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
            _ => {
                return Err(SignerError::UnsupportedKeyType {
                    expected: "ed25519",
                })
            }
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

/// RSA SSH signer.
///
/// Holds the RSA private key (the `rsa` crate's [`rsa::RsaPrivateKey`]) and
/// produces RFC 8332 `rsa-sha2-256` / `rsa-sha2-512` PKCS#1 v1.5 signatures,
/// selected per-sign via [`SignFlags`]. The OpenSSH public key string is
/// precomputed at construction for cheap identity listing.
///
/// # Secret hygiene
///
/// [`rsa::RsaPrivateKey`] implements `ZeroizeOnDrop` (but not `Zeroize`), so it
/// cannot be wrapped in [`Zeroizing`] directly — the same constraint that
/// applies to `ed25519_dalek::SigningKey` in [`Ed25519Signer`]. We rely on the
/// crate's own `ZeroizeOnDrop` impl to wipe the secret CRT components on drop,
/// mirroring the ed25519 approach.
///
/// The PKCS#1 v1.5 padding is deterministic (RFC 8017 §8.2 uses no
/// randomness), so the `rsa` crate's `Signer::<Signature>::try_sign` path
/// (which uses a `DummyRng`) is correct and reproducible.
pub struct RsaSigner {
    /// RSA private key, zeroized on drop via its own `ZeroizeOnDrop` impl.
    key: rsa::RsaPrivateKey,
    /// Precomputed OpenSSH public key string.
    public_ssh: String,
}

impl RsaSigner {
    /// Build a signer from an OpenSSH PEM private key.
    ///
    /// `passphrase` is required when (and only when) the key is
    /// passphrase-protected; supplying a passphrase for an unencrypted key, or
    /// omitting it for an encrypted key, is a loud error. Mirrors
    /// [`Ed25519Signer::from_openssh`].
    pub fn from_openssh(pem: &str, passphrase: Option<&str>) -> SignerResult<Self> {
        let parsed = ssh_key::PrivateKey::from_openssh(pem)
            .map_err(|source| SignerError::ParseKey { source })?;

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
            ssh_key::private::KeypairData::Rsa(kp) => kp,
            _ => return Err(SignerError::UnsupportedKeyType { expected: "rsa" }),
        };

        // Construct `rsa::RsaPrivateKey` directly from the SSH keypair
        // components rather than via the `TryFrom<&RsaKeypair>` impl, because
        // ssh-key 0.6.7's conversion has a bug: it passes `private.p` twice
        // instead of `(p, q)`, which the rsa crate's prime-pairwise-unequal
        // validation rejects as `Error::Crypto`. Build `from_components` with
        // the correct `(p, q)` primes ourselves; `from_components` still runs
        // the rsa crate's own consistency validation (n == p*q, d ≡ e^-1).
        //
        // `rsa::errors::Error` and `ssh_key::Error` are distinct types;
        // `ssh_key` only exposes `Error::Crypto` (a unit variant) for RSA
        // failures, so we lose the precise rsa error string here — accepted
        // trade-off because no key material is in the rsa error.
        let rsa_key = rsa::RsaPrivateKey::from_components(
            rsa::BigUint::try_from(&keypair.public.n)
                .map_err(|source| SignerError::RsaKey { source })?,
            rsa::BigUint::try_from(&keypair.public.e)
                .map_err(|source| SignerError::RsaKey { source })?,
            rsa::BigUint::try_from(&keypair.private.d)
                .map_err(|source| SignerError::RsaKey { source })?,
            vec![
                rsa::BigUint::try_from(&keypair.private.p)
                    .map_err(|source| SignerError::RsaKey { source })?,
                rsa::BigUint::try_from(&keypair.private.q)
                    .map_err(|source| SignerError::RsaKey { source })?,
            ],
        )
        .map_err(|_| SignerError::RsaKey {
            source: ssh_key::Error::Crypto,
        })?;

        // Enforce the 2048-bit minimum (matches ssh-key 0.6.7's
        // `RsaKeypair::MIN_KEY_SIZE`). Reject deliberately weak keys loud
        // rather than silently downgrading or accepting.
        use rsa::traits::PublicKeyParts;
        const MIN_RSA_BITS: usize = 2048;
        if rsa_key.size().saturating_mul(8) < MIN_RSA_BITS {
            return Err(SignerError::RsaKey {
                source: ssh_key::Error::Crypto,
            });
        }
        let public_ssh = key
            .public_key()
            .to_openssh()
            .map_err(|source| SignerError::PublicKey { source })?;

        Ok(Self {
            key: rsa_key,
            public_ssh,
        })
    }
}

impl SshSigner for RsaSigner {
    fn algorithm(&self) -> SshAlgo {
        SshAlgo::Rsa
    }

    fn public_key_ssh(&self) -> SignerResult<String> {
        Ok(self.public_ssh.clone())
    }

    fn sign(&self, data: &[u8], flags: SignFlags) -> SignerResult<Vec<u8>> {
        // RFC 8332 variant selection. SHA-512 wins when both flags are set
        // (matches OpenSSH); default (no flags) is SHA-256 — modern ssh
        // refuses legacy SHA-1 `ssh-rsa`.
        let (alg_name, sig_bytes) = if flags.rsa_sha2_512 {
            (
                "rsa-sha2-512",
                rsa_pkcs1v15_sign::<Sha512>(&self.key, data)?,
            )
        } else {
            (
                "rsa-sha2-256",
                rsa_pkcs1v15_sign::<Sha256>(&self.key, data)?,
            )
        };
        Ok(rsa_wire_signature(alg_name, &sig_bytes))
    }
}

/// Compute a PKCS#1 v1.5 signature over `data` using `key` and digest `D`.
///
/// Builds a transient `pkcs1v15::SigningKey<D>` per call (cloning the private
/// key, which is a bounded BigUint copy) so the same signer can serve either
/// SHA-2 variant from a single stored key. The clone is dropped (and
/// zeroized via `ZeroizeOnDrop`) at the end of the statement.
fn rsa_pkcs1v15_sign<D>(key: &rsa::RsaPrivateKey, data: &[u8]) -> SignerResult<Vec<u8>>
where
    D: Digest + rsa::pkcs8::AssociatedOid,
{
    use rsa::pkcs1v15::SigningKey;
    use rsa::signature::{SignatureEncoding, Signer};

    let signing_key: SigningKey<D> = SigningKey::new(key.clone());
    // Let the rsa crate's `Signer<Signature>` impl hash `data` and apply
    // EMSA-PKCS1-v1_5 padding. PKCS#1 v1.5 is deterministic (RFC 8017 §8.2
    // uses no randomness): the impl uses a `DummyRng`, so no external
    // entropy is required and signing is reproducible.
    let signature = signing_key.sign(data);
    Ok(signature.to_vec())
}

/// Build the SSH agent wire-format signature blob for RSA:
/// `string <alg_name>` + `string <sig bytes>`, where each `string` is a 4-byte
/// big-endian length prefix followed by the bytes. `alg_name` is
/// `rsa-sha2-256` or `rsa-sha2-512`.
fn rsa_wire_signature(alg: &str, sig: &[u8]) -> Vec<u8> {
    let alg_bytes = alg.as_bytes();
    let mut out = Vec::with_capacity(4 + alg_bytes.len() + 4 + sig.len());
    out.extend_from_slice(&(alg_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(alg_bytes);
    out.extend_from_slice(&(sig.len() as u32).to_be_bytes());
    out.extend_from_slice(sig);
    out
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
