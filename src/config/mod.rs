pub mod error;
pub mod general;
pub mod manager;
pub mod notification;
pub mod password;
pub mod security;
pub mod sync;
pub mod validation;
pub mod watcher;

use serde::{Deserialize, Serialize};

pub use error::ConfigError;
pub use general::{AnimationMode, GeneralConfig};
pub use manager::ConfigManager;
pub use notification::{ConfigReloadable, ServiceNotification};
pub use password::PasswordDefaultsConfig;
pub use security::{HealthCheckFrequency, SecurityConfig};
pub use sync::{
    AliyunDriveConfig, AliyunDriveType, AliyunOssConfig, DropboxConfig, GoogleDriveConfig,
    HuaweiObsConfig, OneDriveConfig, ProviderConfig, S3Config, SftpConfig, SftpHostCheck,
    SyncConfig, SyncMode, SyncProvider, TencentCosConfig, UpyunConfig, WebDavConfig,
};
pub use watcher::ConfigWatcher;

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

    pub fn load(vault_dir: &std::path::Path) -> Result<Self, ConfigError> {
        let path = vault_dir.join("config.toml");
        if !path.exists() {
            return Ok(Self::default_config());
        }
        let content = std::fs::read_to_string(&path)?;
        let config = Self::from_toml(&content)?;
        validation::validate(&config)?;
        Ok(config)
    }

    pub fn save(&self, vault_dir: &std::path::Path) -> Result<(), ConfigError> {
        validation::validate(self)?;
        let path = vault_dir.join("config.toml");
        std::fs::create_dir_all(vault_dir)?;
        let toml_str =
            toml::to_string_pretty(self).map_err(|e| ConfigError::Parse(e.to_string()))?;
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

    pub fn from_toml(content: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(content)?)
    }
}
