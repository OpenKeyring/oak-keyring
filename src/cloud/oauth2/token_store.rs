//! Token persistence for OAuth2 authorization.

use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::error::OAuth2Error;

/// OAuth2 token with access and refresh tokens.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OAuth2Token {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub token_type: String,
}

impl OAuth2Token {
    /// Whether the token is expired (with 5-minute safety margin).
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(exp) => Utc::now() + Duration::minutes(5) >= exp,
            None => true,
        }
    }

    /// Whether a refresh_token is available.
    pub fn can_refresh(&self) -> bool {
        self.refresh_token.is_some()
    }
}

/// Stores and loads OAuth2 tokens from the filesystem.
#[derive(Clone)]
pub struct TokenStore {
    base_path: PathBuf,
}

impl TokenStore {
    /// Create a TokenStore using the given directory.
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Save a token to `{base_path}/{provider_id}.token.json`.
    pub fn save(&self, provider_id: &str, token: &OAuth2Token) -> Result<(), OAuth2Error> {
        std::fs::create_dir_all(&self.base_path).map_err(|e| OAuth2Error::TokenStore {
            message: format!(
                "failed to create token directory {}: {}",
                self.base_path.display(),
                e
            ),
        })?;

        let path = self.token_path(provider_id);
        let json = serde_json::to_string_pretty(token).map_err(|e| OAuth2Error::TokenStore {
            message: format!("failed to serialize token: {}", e),
        })?;

        std::fs::write(&path, json).map_err(|e| OAuth2Error::TokenStore {
            message: format!("failed to write token to {}: {}", path.display(), e),
        })?;

        Ok(())
    }

    /// Load a token from `{base_path}/{provider_id}.token.json`.
    /// Returns `None` if the file does not exist.
    pub fn load(&self, provider_id: &str) -> Result<Option<OAuth2Token>, OAuth2Error> {
        let path = self.token_path(provider_id);
        if !path.exists() {
            return Ok(None);
        }

        let json = std::fs::read_to_string(&path).map_err(|e| OAuth2Error::TokenStore {
            message: format!("failed to read token from {}: {}", path.display(), e),
        })?;

        let token: OAuth2Token =
            serde_json::from_str(&json).map_err(|e| OAuth2Error::TokenStore {
                message: format!("failed to deserialize token: {}", e),
            })?;

        Ok(Some(token))
    }

    /// Delete a token file. Returns `true` if deleted, `false` if not found.
    pub fn delete(&self, provider_id: &str) -> Result<bool, OAuth2Error> {
        let path = self.token_path(provider_id);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| OAuth2Error::TokenStore {
                message: format!("failed to delete token {}: {}", path.display(), e),
            })?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn token_path(&self, provider_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.token.json", provider_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_token() -> OAuth2Token {
        OAuth2Token {
            access_token: "test_access_token".to_string(),
            refresh_token: Some("test_refresh_token".to_string()),
            expires_at: Some(Utc::now() + Duration::hours(1)),
            token_type: "Bearer".to_string(),
        }
    }

    #[test]
    fn token_store_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = TokenStore::new(temp_dir.path().to_path_buf());
        let token = make_test_token();

        store.save("test_provider", &token).unwrap();
        let loaded = store.load("test_provider").unwrap().unwrap();

        assert_eq!(loaded.access_token, "test_access_token");
        assert_eq!(loaded.refresh_token, Some("test_refresh_token".to_string()));
    }

    #[test]
    fn token_store_load_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = TokenStore::new(temp_dir.path().to_path_buf());
        let result = store.load("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn token_store_delete_existing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = TokenStore::new(temp_dir.path().to_path_buf());
        let token = make_test_token();

        store.save("test_provider", &token).unwrap();
        let deleted = store.delete("test_provider").unwrap();
        assert!(deleted);

        let loaded = store.load("test_provider").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn token_is_expired_when_expires_at_in_past() {
        let token = OAuth2Token {
            access_token: "expired".to_string(),
            refresh_token: Some("rt".to_string()),
            expires_at: Some(Utc::now() - Duration::hours(1)),
            token_type: "Bearer".to_string(),
        };
        assert!(token.is_expired());
    }

    #[test]
    fn token_is_not_expired_when_fresh() {
        let token = OAuth2Token {
            access_token: "fresh".to_string(),
            refresh_token: Some("rt".to_string()),
            expires_at: Some(Utc::now() + Duration::hours(1)),
            token_type: "Bearer".to_string(),
        };
        assert!(!token.is_expired());
    }

    #[test]
    fn token_can_refresh() {
        let token = OAuth2Token {
            refresh_token: Some("rt".to_string()),
            ..make_test_token()
        };
        assert!(token.can_refresh());

        let token_no_rt = OAuth2Token {
            refresh_token: None,
            ..make_test_token()
        };
        assert!(!token_no_rt.can_refresh());
    }
}
