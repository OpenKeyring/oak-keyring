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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DropboxConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OneDriveConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WebDavConfig {
    pub endpoint: String,
    #[serde(default = "default_root")]
    pub root_path: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub bearer_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SftpConfig {
    pub server: String,
    #[serde(default = "default_root")]
    pub root_path: String,
    pub ssh_key_path: String,
    #[serde(default)]
    pub host_check: SftpHostCheck,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct S3Config {
    pub endpoint: Option<String>,
    pub bucket: String,
    pub region: Option<String>,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AliyunDriveConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default)]
    pub drive_type: AliyunDriveType,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AliyunOssConfig {
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub access_key_secret: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TencentCosConfig {
    pub endpoint: String,
    pub bucket: String,
    pub secret_id: String,
    pub secret_key: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HuaweiObsConfig {
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpyunConfig {
    pub bucket: String,
    pub operator: String,
    pub operator_password: String,
    #[serde(default = "default_root")]
    pub root_path: String,
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
}
