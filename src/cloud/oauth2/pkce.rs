//! PKCE code_verifier and code_challenge generation (S256 method).

use base64::prelude::{Engine as _, BASE64_URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

/// Generate PKCE code_verifier and code_challenge (S256).
///
/// - verifier: 32 bytes random → Base64URL no-pad (43 chars)
/// - challenge: SHA256(verifier) → Base64URL no-pad (43 chars)
pub fn generate_pkce() -> (String, String) {
    let verifier_bytes: [u8; 32] = rand::random();
    let verifier = BASE64_URL_SAFE_NO_PAD.encode(verifier_bytes);
    let challenge = {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        BASE64_URL_SAFE_NO_PAD.encode(hasher.finalize())
    };
    (verifier, challenge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_generates_valid_verifier_and_challenge() {
        let (verifier, challenge) = generate_pkce();

        assert!(
            verifier.len() >= 43,
            "verifier too short: {} chars",
            verifier.len()
        );
        assert!(
            challenge.len() >= 43,
            "challenge too short: {} chars",
            challenge.len()
        );

        let (v2, c2) = generate_pkce();
        assert_ne!(verifier, v2, "verifiers should differ between calls");
        assert_ne!(challenge, c2, "challenges should differ between calls");
    }

    #[test]
    fn pkce_base64url_no_padding() {
        let (verifier, challenge) = generate_pkce();

        assert!(!verifier.contains('='), "verifier must not contain padding");
        assert!(
            !challenge.contains('='),
            "challenge must not contain padding"
        );
    }
}
