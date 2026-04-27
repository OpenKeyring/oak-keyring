//! Google Drive OAuth2 provider.

use super::OAuth2Provider;

const BUILT_IN_CLIENT_ID: &str = env!("OAK_GOOGLE_CLIENT_ID");
const BUILT_IN_CLIENT_SECRET: &str = env!("OAK_GOOGLE_CLIENT_SECRET");

/// Google Drive OAuth2 provider.
#[derive(Debug)]
pub struct GoogleDriveProvider {
    client_id: String,
    client_secret: String,
}

impl Default for GoogleDriveProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleDriveProvider {
    /// Create with built-in compiled credentials.
    pub fn new() -> Self {
        Self {
            client_id: BUILT_IN_CLIENT_ID.to_string(),
            client_secret: BUILT_IN_CLIENT_SECRET.to_string(),
        }
    }

    /// Create with custom credentials (for users with their own Google Cloud project).
    pub fn with_credentials(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
        }
    }
}

impl OAuth2Provider for GoogleDriveProvider {
    fn provider_id(&self) -> &str {
        "google_drive"
    }
    fn display_name(&self) -> &str {
        "Google Drive"
    }
    fn client_id(&self) -> &str {
        &self.client_id
    }
    fn client_secret(&self) -> &str {
        &self.client_secret
    }
    fn auth_url(&self) -> &str {
        "https://accounts.google.com/o/oauth2/auth"
    }
    fn token_url(&self) -> &str {
        "https://oauth2.googleapis.com/token"
    }
    fn scopes(&self) -> &[&str] {
        &["https://www.googleapis.com/auth/drive.appdata"]
    }
}
