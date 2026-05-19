use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SyncProvider {
    #[default]
    Disabled,
    ICloud,
    GoogleDrive,
    Dropbox,
    OneDrive,
    WebDav,
    Sftp,
    S3,
    AliyunDrive,
    AliyunOss,
    TencentCos,
    HuaweiObs,
    Upyun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SyncMode {
    #[default]
    Auto,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SftpHostCheck {
    #[default]
    Strict,
    Accept,
    Add,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AliyunDriveType {
    #[default]
    Default,
    Backup,
    Resource,
}

// -- Provider configs with redacted Debug -----------------------------------

fn redacted(len: usize) -> &'static str {
    if len == 0 {
        "<empty>"
    } else {
        "<redacted>"
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleDriveConfig {
    /// OAuth2 access token -- runtime only, NOT persisted to config.toml.
    #[serde(skip)]
    pub access_token: String,
    /// OAuth2 refresh token -- runtime only, NOT persisted to config.toml.
    #[serde(skip)]
    pub refresh_token: String,
    /// Root directory in Google Drive for sync data.
    #[serde(default = "default_gdrive_root")]
    pub root_path: String,
    /// Deprecated -- credentials are now built-in via build.rs.
    #[deprecated(since = "0.2.0")]
    #[serde(default)]
    pub client_id: String,
    /// Deprecated -- credentials are now built-in via build.rs.
    #[deprecated(since = "0.2.0")]
    #[serde(default)]
    pub client_secret: String,
}

impl fmt::Debug for GoogleDriveConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoogleDriveConfig")
            .field("access_token", &redacted(self.access_token.len()))
            .field("refresh_token", &redacted(self.refresh_token.len()))
            .field("root_path", &self.root_path)
            .finish()
    }
}

fn default_gdrive_root() -> String {
    ".oak-keyring/".to_string()
}

impl Default for GoogleDriveConfig {
    #[allow(deprecated)]
    fn default() -> Self {
        Self {
            access_token: String::new(),
            refresh_token: String::new(),
            root_path: default_gdrive_root(),
            client_id: String::new(),
            client_secret: String::new(),
        }
    }
}

#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DropboxConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

impl fmt::Debug for DropboxConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DropboxConfig")
            .field("client_id", &redacted(self.client_id.len()))
            .field("client_secret", &redacted(self.client_secret.len()))
            .field("refresh_token", &redacted(self.refresh_token.len()))
            .field("root_path", &self.root_path)
            .finish()
    }
}

#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OneDriveConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

impl fmt::Debug for OneDriveConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OneDriveConfig")
            .field("client_id", &redacted(self.client_id.len()))
            .field("client_secret", &redacted(self.client_secret.len()))
            .field("refresh_token", &redacted(self.refresh_token.len()))
            .field("root_path", &self.root_path)
            .finish()
    }
}

#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WebDavConfig {
    pub endpoint: String,
    #[serde(default = "default_root")]
    pub root_path: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub bearer_token: Option<String>,
}

impl fmt::Debug for WebDavConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebDavConfig")
            .field("endpoint", &self.endpoint)
            .field("root_path", &self.root_path)
            .field("username", &self.username)
            .field(
                "password",
                &self.password.as_ref().map(|s| redacted(s.len())),
            )
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|s| redacted(s.len())),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SftpConfig {
    pub server: String,
    #[serde(default = "default_root")]
    pub root_path: String,
    pub ssh_key_path: String,
    #[serde(default)]
    pub host_check: SftpHostCheck,
}

impl fmt::Debug for SftpConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SftpConfig")
            .field("server", &self.server)
            .field("root_path", &self.root_path)
            .field("ssh_key_path", &self.ssh_key_path)
            .field("host_check", &self.host_check)
            .finish()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct S3Config {
    pub endpoint: Option<String>,
    pub bucket: String,
    pub region: Option<String>,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

impl fmt::Debug for S3Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Config")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("access_key_id", &redacted(self.access_key_id.len()))
            .field("secret_access_key", &redacted(self.secret_access_key.len()))
            .field("root_path", &self.root_path)
            .finish()
    }
}

#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AliyunDriveConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default)]
    pub drive_type: AliyunDriveType,
    #[serde(default = "default_root")]
    pub root_path: String,
}

impl fmt::Debug for AliyunDriveConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AliyunDriveConfig")
            .field("client_id", &redacted(self.client_id.len()))
            .field("client_secret", &redacted(self.client_secret.len()))
            .field("refresh_token", &redacted(self.refresh_token.len()))
            .field("drive_type", &self.drive_type)
            .field("root_path", &self.root_path)
            .finish()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct AliyunOssConfig {
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub access_key_secret: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

impl fmt::Debug for AliyunOssConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AliyunOssConfig")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("access_key_id", &redacted(self.access_key_id.len()))
            .field("access_key_secret", &redacted(self.access_key_secret.len()))
            .field("root_path", &self.root_path)
            .finish()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct TencentCosConfig {
    pub endpoint: String,
    pub bucket: String,
    pub secret_id: String,
    pub secret_key: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

impl fmt::Debug for TencentCosConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TencentCosConfig")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("secret_id", &redacted(self.secret_id.len()))
            .field("secret_key", &redacted(self.secret_key.len()))
            .field("root_path", &self.root_path)
            .finish()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct HuaweiObsConfig {
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

impl fmt::Debug for HuaweiObsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HuaweiObsConfig")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("access_key_id", &redacted(self.access_key_id.len()))
            .field("secret_access_key", &redacted(self.secret_access_key.len()))
            .field("root_path", &self.root_path)
            .finish()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct UpyunConfig {
    pub bucket: String,
    pub operator: String,
    pub operator_password: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

impl fmt::Debug for UpyunConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpyunConfig")
            .field("bucket", &self.bucket)
            .field("operator", &self.operator)
            .field("operator_password", &redacted(self.operator_password.len()))
            .field("root_path", &self.root_path)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderConfig {
    ICloud,
    GoogleDrive(GoogleDriveConfig),
    Dropbox(DropboxConfig),
    OneDrive(OneDriveConfig),
    WebDav(WebDavConfig),
    Sftp(SftpConfig),
    S3(S3Config),
    AliyunDrive(AliyunDriveConfig),
    AliyunOss(AliyunOssConfig),
    TencentCos(TencentCosConfig),
    HuaweiObs(HuaweiObsConfig),
    Upyun(UpyunConfig),
}

fn default_root() -> String {
    "/".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default)]
    pub provider: SyncProvider,
    #[serde(default)]
    pub sync_mode: SyncMode,
    #[serde(default = "default_interval")]
    pub auto_interval_seconds: u64,
    #[serde(default)]
    pub provider_config: Option<ProviderConfig>,
}

fn default_interval() -> u64 {
    600
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            provider: SyncProvider::Disabled,
            sync_mode: SyncMode::Auto,
            auto_interval_seconds: 600,
            provider_config: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn gdrive_config_tokens_not_serialized() {
        let cfg = GoogleDriveConfig {
            access_token: "secret_access".to_string(),
            refresh_token: "secret_refresh".to_string(),
            root_path: default_gdrive_root(),
            client_id: String::new(),
            client_secret: String::new(),
        };
        let toml_str = toml::to_string(&cfg).unwrap();
        assert!(!toml_str.contains("secret_access"));
        assert!(!toml_str.contains("secret_refresh"));
        assert!(toml_str.contains(".oak-keyring/"));
    }

    #[test]
    fn gdrive_config_deserialize_without_tokens() {
        let toml_str = "root_path = \".oak-keyring/\"\n";
        let cfg: GoogleDriveConfig = toml::from_str(toml_str).unwrap();
        assert!(cfg.access_token.is_empty());
        assert!(cfg.refresh_token.is_empty());
        assert_eq!(cfg.root_path, ".oak-keyring/");
    }

    #[test]
    fn provider_config_debug_redacts_secrets() {
        let s3 = S3Config {
            endpoint: Some("https://s3.amazonaws.com".to_string()),
            bucket: "my-bucket".to_string(),
            region: Some("us-east-1".to_string()),
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            root_path: "/".to_string(),
        };
        let debug_output = format!("{:?}", s3);
        assert!(
            !debug_output.contains("AKIAIOSFODNN7EXAMPLE"),
            "Debug must not expose access_key_id"
        );
        assert!(
            !debug_output.contains("wJalrXUtnFEMI"),
            "Debug must not expose secret_access_key"
        );
        assert!(
            debug_output.contains("<redacted>"),
            "Debug must show <redacted> for secret fields"
        );
        assert!(
            debug_output.contains("my-bucket"),
            "Debug must show non-sensitive fields"
        );
    }

    #[test]
    fn webdav_config_debug_redacts_password() {
        let cfg = WebDavConfig {
            endpoint: "https://dav.example.com".to_string(),
            root_path: "/".to_string(),
            username: Some("user".to_string()),
            password: Some("s3cret".to_string()),
            bearer_token: None,
        };
        let debug_output = format!("{:?}", cfg);
        assert!(
            !debug_output.contains("s3cret"),
            "Debug must not expose password"
        );
        assert!(debug_output.contains("user"), "Debug must show username");
    }

    #[test]
    fn dropbox_config_debug_redacts_tokens() {
        let cfg = DropboxConfig {
            client_id: "cid".to_string(),
            client_secret: "cs".to_string(),
            refresh_token: "rt".to_string(),
            root_path: "/".to_string(),
        };
        let debug_output = format!("{:?}", cfg);
        assert!(!debug_output.contains("cid"));
        assert!(!debug_output.contains("cs"));
        assert!(!debug_output.contains("rt"));
    }
}
