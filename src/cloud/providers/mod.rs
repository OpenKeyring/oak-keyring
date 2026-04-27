//! OAuth2 provider trait and provider implementations.

mod google;

pub use google::GoogleDriveProvider;

/// Trait for OAuth2 providers (Google Drive, OneDrive, Dropbox, etc.).
pub trait OAuth2Provider: Send + Sync {
    /// Unique provider identifier, e.g. "google_drive".
    fn provider_id(&self) -> &str;

    /// Human-readable display name, e.g. "Google Drive".
    fn display_name(&self) -> &str;

    /// OAuth2 Client ID.
    fn client_id(&self) -> &str;

    /// OAuth2 Client Secret.
    fn client_secret(&self) -> &str;

    /// Authorization endpoint URL.
    fn auth_url(&self) -> &str;

    /// Token exchange endpoint URL.
    fn token_url(&self) -> &str;

    /// OAuth2 scopes to request.
    fn scopes(&self) -> &[&str];

    /// Redirect URI (defaults to localhost:8879).
    fn redirect_uri(&self) -> &str {
        "http://localhost:8879"
    }
}
