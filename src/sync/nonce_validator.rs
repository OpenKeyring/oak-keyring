//! NonceValidator for vault identity token validation during sync.
//!
//! This module provides stateless validation of vault identity tokens
//! to ensure sync operations only proceed between matching vaults.

use crate::errors::mapping::sync::SyncError;
use rand::TryRngCore;

/// Represents the action to take based on vault identity validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityAction {
    /// Both tokens exist and match — allow sync to proceed.
    AllowSync,
    /// Local token is None but remote exists — adopt the remote token.
    AdoptRemoteToken(String),
    /// No token exists locally (first sync) — generate a new token.
    GenerateNewToken,
}

/// Stateless validator for vault identity tokens.
pub struct NonceValidator;

impl NonceValidator {
    /// Validates local and remote vault identity tokens and determines the appropriate action.
    ///
    /// # Arguments
    /// * `local_token` - The local vault identity token (if any).
    /// * `remote_token` - The remote vault identity token (if any).
    ///
    /// # Returns
    /// * `Ok(IdentityAction::AllowSync)` - Both tokens exist and match.
    /// * `Ok(IdentityAction::AdoptRemoteToken)` - Local is None, remote exists.
    /// * `Ok(IdentityAction::GenerateNewToken)` - No remote token (first sync) or both None.
    /// * `Err(SyncError::VaultIdentityMismatch)` - Both tokens exist but differ.
    pub fn validate_identity(
        local_token: Option<&str>,
        remote_token: Option<&str>,
    ) -> Result<IdentityAction, SyncError> {
        match (local_token, remote_token) {
            // Both tokens exist and match — allow sync
            (Some(local), Some(remote)) if local == remote => Ok(IdentityAction::AllowSync),
            // Both tokens exist but differ — mismatch error
            (Some(local), Some(remote)) => Err(SyncError::VaultIdentityMismatch {
                local_token: local.to_string(),
                remote_token: remote.to_string(),
            }),
            // Local is None, remote exists — adopt remote token
            (None, Some(remote)) => Ok(IdentityAction::AdoptRemoteToken(remote.to_string())),
            // Remote is None (first sync to cloud) — generate new token
            (Some(_), None) => Ok(IdentityAction::GenerateNewToken),
            // Both None — generate new token
            (None, None) => Ok(IdentityAction::GenerateNewToken),
        }
    }

    /// Generates a new random vault identity token.
    ///
    /// Generates a 24-byte random value encoded as a 48-character hex string.
    ///
    /// # Returns
    /// A 48-character hex string representing the new vault identity token.
    pub fn generate_token() -> Result<String, SyncError> {
        let mut bytes = [0u8; 24];
        rand::rngs::OsRng
            .try_fill_bytes(&mut bytes)
            .map_err(|e| SyncError::ProviderError {
                provider: "rng".to_string(),
                message: e.to_string(),
            })?;
        Ok(hex::encode(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_both_match() {
        let result = NonceValidator::validate_identity(Some("token_abc123"), Some("token_abc123"));
        assert!(matches!(result, Ok(IdentityAction::AllowSync)));
    }

    #[test]
    fn validate_mismatch() {
        let result = NonceValidator::validate_identity(Some("token_local"), Some("token_remote"));
        assert!(matches!(
            result,
            Err(SyncError::VaultIdentityMismatch { .. })
        ));
        if let Err(SyncError::VaultIdentityMismatch {
            local_token,
            remote_token,
        }) = result
        {
            assert_eq!(local_token, "token_local");
            assert_eq!(remote_token, "token_remote");
        }
    }

    #[test]
    fn validate_local_none() {
        let result = NonceValidator::validate_identity(None, Some("remote_token"));
        match result {
            Ok(IdentityAction::AdoptRemoteToken(ref token)) => {
                assert_eq!(token, "remote_token");
            }
            _ => panic!("expected AdoptRemoteToken"),
        }
    }

    #[test]
    fn validate_remote_none() {
        let result = NonceValidator::validate_identity(Some("local_token"), None);
        assert!(matches!(result, Ok(IdentityAction::GenerateNewToken)));
    }

    #[test]
    fn validate_both_none() {
        let result = NonceValidator::validate_identity(None, None);
        assert!(matches!(result, Ok(IdentityAction::GenerateNewToken)));
    }

    #[test]
    fn generate_token_length() {
        let token = NonceValidator::generate_token().expect("generate_token should succeed");
        assert_eq!(token.len(), 48);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_token_unique() {
        let token1 = NonceValidator::generate_token().expect("generate_token should succeed");
        let token2 = NonceValidator::generate_token().expect("generate_token should succeed");
        assert_ne!(token1, token2);
    }

    #[test]
    fn identity_action_debug() {
        let action = IdentityAction::AllowSync;
        assert_eq!(format!("{:?}", action), "AllowSync");
    }

    #[test]
    fn identity_action_clone() {
        let action = IdentityAction::AdoptRemoteToken("test".to_string());
        let cloned = action.clone();
        assert_eq!(action, cloned);
    }

    #[test]
    fn identity_action_partial_eq() {
        assert_eq!(IdentityAction::AllowSync, IdentityAction::AllowSync);
        assert_eq!(
            IdentityAction::AdoptRemoteToken("test".to_string()),
            IdentityAction::AdoptRemoteToken("test".to_string())
        );
        assert_ne!(IdentityAction::AllowSync, IdentityAction::GenerateNewToken);
    }
}
