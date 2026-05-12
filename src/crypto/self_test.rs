//! Startup self-test for oak-keyring's core AEAD primitive.
//!
//! This is intentionally narrow: it verifies XChaCha20-Poly1305 with an
//! external fixed vector before any vault operation can run. It does not try
//! to cover every crypto dependency.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

use super::xchacha20::{self, EncryptedData};

#[derive(Debug)]
pub enum CryptoSelfTestError {
    XChaCha20Poly1305 { detail: String },
}

impl std::fmt::Display for CryptoSelfTestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::XChaCha20Poly1305 { detail } => {
                write!(f, "XChaCha20-Poly1305: {detail}")
            }
        }
    }
}

impl std::error::Error for CryptoSelfTestError {}

pub fn run_all() -> Result<(), CryptoSelfTestError> {
    test_xchacha20_poly1305()
}

// Vector source: draft-irtf-cfrg-xchacha-03 Appendix A.1. RustCrypto's
// chacha20poly1305 0.10.1 tests/lib.rs also uses this same vector.
// XChaCha20-Poly1305 has no RFC-level KAT, so keep this source note with the
// constants.
const KEY: [u8; 32] = [
    0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f,
    0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f,
];

const NONCE: [u8; 24] = [
    0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f,
    0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57,
];

const AAD: [u8; 12] = [
    0x50, 0x51, 0x52, 0x53, 0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7,
];

const PLAINTEXT: &[u8] = b"Ladies and Gentlemen of the class of '99: \
    If I could offer you only one tip for the future, sunscreen would be it.";

const CIPHERTEXT: &[u8] = &[
    0xbd, 0x6d, 0x17, 0x9d, 0x3e, 0x83, 0xd4, 0x3b, 0x95, 0x76, 0x57, 0x94, 0x93, 0xc0, 0xe9, 0x39,
    0x57, 0x2a, 0x17, 0x00, 0x25, 0x2b, 0xfa, 0xcc, 0xbe, 0xd2, 0x90, 0x2c, 0x21, 0x39, 0x6c, 0xbb,
    0x73, 0x1c, 0x7f, 0x1b, 0x0b, 0x4a, 0xa6, 0x44, 0x0b, 0xf3, 0xa8, 0x2f, 0x4e, 0xda, 0x7e, 0x39,
    0xae, 0x64, 0xc6, 0x70, 0x8c, 0x54, 0xc2, 0x16, 0xcb, 0x96, 0xb7, 0x2e, 0x12, 0x13, 0xb4, 0x52,
    0x2f, 0x8c, 0x9b, 0xa4, 0x0d, 0xb5, 0xd9, 0x45, 0xb1, 0x1b, 0x69, 0xb9, 0x82, 0xc1, 0xbb, 0x9e,
    0x3f, 0x3f, 0xac, 0x2b, 0xc3, 0x69, 0x48, 0x8f, 0x76, 0xb2, 0x38, 0x35, 0x65, 0xd3, 0xff, 0xf9,
    0x21, 0xf9, 0x66, 0x4c, 0x97, 0x63, 0x7d, 0xa9, 0x76, 0x88, 0x12, 0xf6, 0x15, 0xc6, 0x8b, 0x13,
    0xb5, 0x2e,
];

const TAG: &[u8] = &[
    0xc0, 0x87, 0x59, 0x24, 0xc1, 0xc7, 0x98, 0x79, 0x47, 0xde, 0xaf, 0xd8, 0x78, 0x0a, 0xcf, 0x49,
];

fn test_xchacha20_poly1305() -> Result<(), CryptoSelfTestError> {
    let cipher = XChaCha20Poly1305::new_from_slice(&KEY).map_err(|_| {
        CryptoSelfTestError::XChaCha20Poly1305 {
            detail: "cipher init failed".into(),
        }
    })?;

    let encrypted = cipher
        .encrypt(
            XNonce::from_slice(&NONCE),
            Payload {
                msg: PLAINTEXT,
                aad: &AAD,
            },
        )
        .map_err(|_| CryptoSelfTestError::XChaCha20Poly1305 {
            detail: "encrypt failed".into(),
        })?;

    let expected = expected_ciphertext_and_tag();
    if encrypted != expected {
        return Err(CryptoSelfTestError::XChaCha20Poly1305 {
            detail: "ciphertext mismatch".into(),
        });
    }

    let decrypted = xchacha20::decrypt_with_aad(
        &EncryptedData {
            ciphertext: expected,
            nonce: NONCE,
        },
        &AAD,
        &KEY,
    )
    .map_err(|_| CryptoSelfTestError::XChaCha20Poly1305 {
        detail: "decrypt failed".into(),
    })?;

    if decrypted != PLAINTEXT {
        return Err(CryptoSelfTestError::XChaCha20Poly1305 {
            detail: "plaintext mismatch".into(),
        });
    }

    Ok(())
}

fn expected_ciphertext_and_tag() -> Vec<u8> {
    let mut expected = Vec::with_capacity(CIPHERTEXT.len() + TAG.len());
    expected.extend_from_slice(CIPHERTEXT);
    expected.extend_from_slice(TAG);
    expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_self_test_passes() {
        run_all().expect("startup crypto self-test should pass");
    }

    #[test]
    fn expected_vector_has_poly1305_tag_appended() {
        let expected = expected_ciphertext_and_tag();
        assert_eq!(expected.len(), PLAINTEXT.len() + 16);
        assert_eq!(&expected[..CIPHERTEXT.len()], CIPHERTEXT);
        assert_eq!(&expected[CIPHERTEXT.len()..], TAG);
    }
}
