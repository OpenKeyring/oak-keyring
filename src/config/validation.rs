use super::error::ConfigError;
use super::sync::{ProviderConfig, SyncProvider};
use super::AppConfig;

pub fn validate(config: &AppConfig) -> Result<(), ConfigError> {
    validate_provider_consistency(config)?;
    Ok(())
}

fn validate_provider_consistency(config: &AppConfig) -> Result<(), ConfigError> {
    let provider = config.sync.provider;

    // When disabled, any stale provider_config is acceptable
    if provider == SyncProvider::Disabled {
        return Ok(());
    }

    // When provider is active, provider_config must match
    match (&config.sync.provider_config, provider) {
        (None, _) => {
            // No provider_config — sync won't work but not a validation error per spec
            Ok(())
        }
        (Some(pc), p) => {
            if !provider_config_matches(pc, p) {
                Err(ConfigError::Validation(format!(
                    "sync.provider is {:?} but provider_config is {} — they must match",
                    p,
                    provider_config_variant_name(pc),
                )))
            } else {
                Ok(())
            }
        }
    }
}

fn provider_config_matches(pc: &ProviderConfig, provider: SyncProvider) -> bool {
    matches!(
        (pc, provider),
        (ProviderConfig::ICloud, SyncProvider::ICloud)
            | (ProviderConfig::GoogleDrive(_), SyncProvider::GoogleDrive)
            | (ProviderConfig::Dropbox(_), SyncProvider::Dropbox)
            | (ProviderConfig::OneDrive(_), SyncProvider::OneDrive)
            | (ProviderConfig::WebDav(_), SyncProvider::WebDav)
            | (ProviderConfig::Sftp(_), SyncProvider::Sftp)
            | (ProviderConfig::S3(_), SyncProvider::S3)
            | (ProviderConfig::AliyunDrive(_), SyncProvider::AliyunDrive)
            | (ProviderConfig::AliyunOss(_), SyncProvider::AliyunOss)
            | (ProviderConfig::TencentCos(_), SyncProvider::TencentCos)
            | (ProviderConfig::HuaweiObs(_), SyncProvider::HuaweiObs)
            | (ProviderConfig::Upyun(_), SyncProvider::Upyun)
    )
}

fn provider_config_variant_name(pc: &ProviderConfig) -> &'static str {
    match pc {
        ProviderConfig::ICloud => "ICloud",
        ProviderConfig::GoogleDrive(_) => "GoogleDrive",
        ProviderConfig::Dropbox(_) => "Dropbox",
        ProviderConfig::OneDrive(_) => "OneDrive",
        ProviderConfig::WebDav(_) => "WebDav",
        ProviderConfig::Sftp(_) => "Sftp",
        ProviderConfig::S3(_) => "S3",
        ProviderConfig::AliyunDrive(_) => "AliyunDrive",
        ProviderConfig::AliyunOss(_) => "AliyunOss",
        ProviderConfig::TencentCos(_) => "TencentCos",
        ProviderConfig::HuaweiObs(_) => "HuaweiObs",
        ProviderConfig::Upyun(_) => "Upyun",
    }
}
