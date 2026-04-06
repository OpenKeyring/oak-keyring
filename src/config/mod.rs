pub mod general;
pub mod password;
pub mod security;
pub mod sync;

use serde::{Deserialize, Serialize};

pub use general::{AnimationMode, GeneralConfig};
pub use password::PasswordDefaultsConfig;
pub use security::{HealthCheckFrequency, SecurityConfig};
pub use sync::{
    AliyunDriveConfig, AliyunDriveType, AliyunOssConfig, DropboxConfig, GoogleDriveConfig,
    HuaweiObsConfig, OneDriveConfig, S3Config, SftpConfig, SftpHostCheck, SyncConfig, SyncMode,
    SyncProvider, TencentCosConfig, UpyunConfig, WebDavConfig,
};

#[cfg(test)]
mod config_test;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub password: PasswordDefaultsConfig,
}

impl AppConfig {
    pub fn default_config() -> Self {
        Self::default()
    }

    pub fn load(vault_dir: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let path = vault_dir.join("config.toml");
        if !path.exists() {
            return Ok(Self::default_config());
        }
        let content = std::fs::read_to_string(&path)?;
        Self::from_toml(&content)
    }

    pub fn save(&self, vault_dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        let path = vault_dir.join("config.toml");
        std::fs::create_dir_all(vault_dir)?;
        let toml_str = toml::to_string_pretty(self)?;
        let tmp_path = path.with_extension("toml.tmp");
        std::fs::write(&tmp_path, &toml_str)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&tmp_path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&tmp_path, perms)?;
        }
        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    pub fn from_toml(content: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(toml::from_str(content)?)
    }
}
