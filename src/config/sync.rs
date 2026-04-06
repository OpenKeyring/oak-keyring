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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoogleDriveConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DropboxConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OneDriveConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebDavConfig {
    pub endpoint: String,
    #[serde(default = "default_root")]
    pub root_path: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub bearer_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpConfig {
    pub server: String,
    #[serde(default = "default_root")]
    pub root_path: String,
    pub ssh_key_path: String,
    #[serde(default)]
    pub host_check: SftpHostCheck,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub endpoint: Option<String>,
    pub bucket: String,
    pub region: Option<String>,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AliyunDriveConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default)]
    pub drive_type: AliyunDriveType,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliyunOssConfig {
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub access_key_secret: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TencentCosConfig {
    pub endpoint: String,
    pub bucket: String,
    pub secret_id: String,
    pub secret_key: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuaweiObsConfig {
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpyunConfig {
    pub bucket: String,
    pub operator: String,
    pub operator_password: String,
    #[serde(default = "default_root")]
    pub root_path: String,
}

fn default_root() -> String {
    "/".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default)]
    pub provider: SyncProvider,
    #[serde(default)]
    pub sync_mode: SyncMode,
    #[serde(default = "default_interval")]
    pub auto_interval_seconds: u64,
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
        }
    }
}
