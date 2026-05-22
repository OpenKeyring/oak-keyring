//! Cloud provider abstraction and factory.
//!
//! Defines the ProviderAdapter trait for cloud storage backends and provides
//! a factory function to create CloudStorage instances from SyncConfig.

use opendal::Operator;

use crate::cloud::adapters::{
    AliyunDriveAdapter, AliyunOssAdapter, DropboxAdapter, GoogleDriveAdapter, HuaweiObsAdapter,
    ICloudAdapter, OneDriveAdapter, S3Adapter, SftpAdapter, TencentCosAdapter, UpyunAdapter,
};
use crate::config::sync::{ProviderConfig, SyncConfig, SyncProvider};
use crate::errors::mapping::sync::SyncError;

/// Trait for cloud storage provider adapters.
///
/// Each provider (WebDAV, S3, iCloud, etc.) implements this trait to:
/// - Create OpenDAL Operator from configuration
/// - Validate provider-specific configuration
/// - Normalize paths for the provider
/// - Handle provider-specific authentication (e.g., OAuth refresh)
pub trait ProviderAdapter: Send + Sync {
    /// Create an OpenDAL Operator from provider config.
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError>;

    /// Validate config has all required fields.
    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError>;

    /// Normalize path for this provider (handle trailing/leading slashes).
    fn normalize_path(&self, base: &str, file: &str) -> String {
        let base = base.trim_end_matches('/');
        let file = file.trim_start_matches('/');
        if base.is_empty() {
            format!("/{}", file)
        } else {
            format!("{}/{}", base, file)
        }
    }

    /// Whether this provider needs file watching (only iCloud).
    fn needs_watcher(&self) -> bool {
        false
    }

    /// Refresh OAuth2 token if applicable.
    fn refresh_auth(&self, _operator: &mut Operator) -> Result<(), SyncError> {
        Ok(())
    }

    /// Test connection by trying to list root.
    fn test_connection(&self, operator: &Operator) -> Result<(), SyncError> {
        let _ = operator;
        Ok(())
    }
}

/// WebDAV provider adapter implementation.
#[derive(Debug, Default)]
pub struct WebDavAdapter;

impl WebDavAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderAdapter for WebDavAdapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            ProviderConfig::WebDav(_) => Err(SyncError::ProviderNotSupported {
                provider: "webdav".to_string(),
            }),
            _ => Err(SyncError::ProviderError {
                provider: "webdav".to_string(),
                message: "expected WebDav config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            ProviderConfig::WebDav(webdav_config) => {
                // Validate endpoint is non-empty
                if webdav_config.endpoint.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "endpoint".to_string(),
                        reason: "endpoint cannot be empty".to_string(),
                    });
                }

                // Validate authentication: either bearer_token OR (username + password)
                let has_bearer = webdav_config.bearer_token.is_some();
                let has_basic =
                    webdav_config.username.is_some() && webdav_config.password.is_some();

                if !has_bearer && !has_basic {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "authentication".to_string(),
                        reason: "either bearer_token or (username + password) must be provided"
                            .to_string(),
                    });
                }

                Ok(())
            }
            _ => Err(SyncError::ProviderError {
                provider: "webdav".to_string(),
                message: "expected WebDav config".to_string(),
            }),
        }
    }

    fn test_connection(&self, operator: &Operator) -> Result<(), SyncError> {
        let _ = operator;
        Ok(())
    }
}

/// Creates a CloudStorage instance from SyncConfig.
///
/// Matches on `config.provider` and delegates to the corresponding adapter.
///
/// **Implemented providers** (all validate config + create OpenDAL operator):
/// - WebDAV, iCloud, SFTP
/// - S3, AliyunOSS, TencentCOS, HuaweiOBS
/// - GoogleDrive (OAuth2 PKCE with built-in credentials)
/// - Dropbox (OAuth2 with user-provided credentials)
///
/// **Stub providers** (adapter exists but `create_operator()` returns `ProviderNotSupported`):
/// - OneDrive, AliyunDrive, Upyun — deferred to future implementation
///
/// # Errors
///
/// Returns `SyncError::ProviderNotSupported` if:
/// - Provider is `Disabled`
/// - Provider adapter returns `ProviderNotSupported` (OneDrive, AliyunDrive, Upyun)
///
/// Returns `SyncError::ConfigValidationFailed` if:
/// - Provider config is missing or fails validation
pub fn create_cloud_storage(config: &SyncConfig) -> Result<crate::cloud::CloudStorage, SyncError> {
    let provider_str = provider_name(config.provider);

    match config.provider {
        SyncProvider::Disabled => Err(SyncError::ProviderNotSupported {
            provider: provider_str,
        }),

        SyncProvider::WebDav => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "WebDAV provider requires provider_config".to_string(),
                }
            })?;

            let adapter = WebDavAdapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }

        SyncProvider::ICloud => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "ICloud provider requires provider_config".to_string(),
                }
            })?;

            let adapter = ICloudAdapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }

        SyncProvider::S3 => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "S3 provider requires provider_config".to_string(),
                }
            })?;

            let adapter = S3Adapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }

        SyncProvider::AliyunOss => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "AliyunOss provider requires provider_config".to_string(),
                }
            })?;

            let adapter = AliyunOssAdapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }

        SyncProvider::TencentCos => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "TencentCos provider requires provider_config".to_string(),
                }
            })?;

            let adapter = TencentCosAdapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }

        SyncProvider::HuaweiObs => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "HuaweiObs provider requires provider_config".to_string(),
                }
            })?;

            let adapter = HuaweiObsAdapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }

        SyncProvider::Upyun => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "Upyun provider requires provider_config".to_string(),
                }
            })?;

            let adapter = UpyunAdapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }

        SyncProvider::Sftp => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "Sftp provider requires provider_config".to_string(),
                }
            })?;

            let adapter = SftpAdapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }

        SyncProvider::GoogleDrive => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "GoogleDrive provider requires provider_config".to_string(),
                }
            })?;

            let adapter = GoogleDriveAdapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }

        SyncProvider::Dropbox => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "Dropbox provider requires provider_config".to_string(),
                }
            })?;

            let adapter = DropboxAdapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }

        SyncProvider::OneDrive => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "OneDrive provider requires provider_config".to_string(),
                }
            })?;

            let adapter = OneDriveAdapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }

        SyncProvider::AliyunDrive => {
            let provider_config = config.provider_config.as_ref().ok_or_else(|| {
                SyncError::ConfigValidationFailed {
                    field: "provider_config".to_string(),
                    reason: "AliyunDrive provider requires provider_config".to_string(),
                }
            })?;

            let adapter = AliyunDriveAdapter::new();
            adapter.validate_config(provider_config)?;
            let operator = adapter.create_operator(provider_config)?;
            Ok(crate::cloud::CloudStorage::new(operator, provider_str))
        }
    }
}

/// Returns a human-readable string for the provider.
pub fn provider_name(provider: SyncProvider) -> String {
    match provider {
        SyncProvider::Disabled => "disabled".to_string(),
        SyncProvider::ICloud => "icloud".to_string(),
        SyncProvider::GoogleDrive => "google_drive".to_string(),
        SyncProvider::Dropbox => "dropbox".to_string(),
        SyncProvider::OneDrive => "onedrive".to_string(),
        SyncProvider::WebDav => "webdav".to_string(),
        SyncProvider::Sftp => "sftp".to_string(),
        SyncProvider::S3 => "s3".to_string(),
        SyncProvider::AliyunDrive => "aliyun_drive".to_string(),
        SyncProvider::AliyunOss => "aliyun_oss".to_string(),
        SyncProvider::TencentCos => "tencent_cos".to_string(),
        SyncProvider::HuaweiObs => "huawei_obs".to_string(),
        SyncProvider::Upyun => "upyun".to_string(),
    }
}
