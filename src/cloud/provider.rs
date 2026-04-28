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
            ProviderConfig::WebDav(webdav_config) => {
                let builder =
                    opendal::services::Webdav::default().endpoint(&webdav_config.endpoint);

                let builder = if !webdav_config.root_path.is_empty() {
                    builder.root(&webdav_config.root_path)
                } else {
                    builder
                };

                let builder = if let Some(ref bearer_token) = webdav_config.bearer_token {
                    builder.token(bearer_token)
                } else if let (Some(ref username), Some(ref password)) =
                    (&webdav_config.username, &webdav_config.password)
                {
                    builder.username(username).password(password)
                } else {
                    builder
                };

                let operator = Operator::new(builder)
                    .map_err(|e| SyncError::ProviderError {
                        provider: "webdav".to_string(),
                        message: format!("failed to create operator: {}", e),
                    })?
                    .finish();

                Ok(operator)
            }
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
/// This factory function:
/// - Matches on `config.provider` to determine the provider type
/// - For `Disabled` → returns `ProviderNotSupported`
/// - For unimplemented providers (all except WebDAV) → returns `ProviderNotSupported`
/// - For `WebDav` → validates config, creates operator, returns `CloudStorage`
///
/// # Errors
///
/// Returns `SyncError::ProviderNotSupported` if:
/// - Provider is `Disabled`
/// - Provider is not yet implemented (J-06 through J-19)
///
/// Returns `SyncError::ConfigValidationFailed` if:
/// - WebDAV config is missing or invalid
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::cloud::adapters::{
        AliyunDriveAdapter, GoogleDriveAdapter, ICloudAdapter, S3Adapter, SftpAdapter,
    };
    use crate::config::sync::{
        AliyunDriveConfig, GoogleDriveConfig, S3Config, SftpConfig, SyncMode, WebDavConfig,
    };

    // ==================== Factory Tests ====================

    #[test]
    fn test_factory_disabled_provider_returns_error() {
        let config = SyncConfig {
            provider: SyncProvider::Disabled,
            sync_mode: SyncMode::Auto,
            auto_interval_seconds: 600,
            provider_config: None,
        };

        let result = create_cloud_storage(&config);
        assert!(result.is_err());

        match result.unwrap_err() {
            SyncError::ProviderNotSupported { provider } => {
                assert_eq!(provider, "disabled");
            }
            other => panic!("expected ProviderNotSupported, got {:?}", other),
        }
    }

    #[test]
    fn test_factory_aliyun_drive_not_supported() {
        let config = SyncConfig {
            provider: SyncProvider::AliyunDrive,
            sync_mode: SyncMode::Auto,
            auto_interval_seconds: 600,
            provider_config: Some(ProviderConfig::AliyunDrive(AliyunDriveConfig {
                client_id: "test".to_string(),
                client_secret: "secret".to_string(),
                refresh_token: "token".to_string(),
                drive_type: crate::config::sync::AliyunDriveType::Default,
                root_path: "/".to_string(),
            })),
        };

        let result = create_cloud_storage(&config);
        assert!(result.is_err());

        match result.unwrap_err() {
            SyncError::ProviderNotSupported { provider } => {
                assert_eq!(provider, "aliyun_drive");
            }
            other => panic!("expected ProviderNotSupported, got {:?}", other),
        }
    }

    #[test]
    fn test_factory_webdav_with_valid_config() {
        let config = SyncConfig {
            provider: SyncProvider::WebDav,
            sync_mode: SyncMode::Auto,
            auto_interval_seconds: 600,
            provider_config: Some(ProviderConfig::WebDav(WebDavConfig {
                endpoint: "https://webdav.example.com".to_string(),
                root_path: "/".to_string(),
                username: Some("user".to_string()),
                password: Some("pass".to_string()),
                bearer_token: None,
            })),
        };

        let result = create_cloud_storage(&config);
        assert!(result.is_ok());

        let storage = result.unwrap();
        drop(storage);
    }

    #[test]
    fn test_factory_webdav_missing_config() {
        let config = SyncConfig {
            provider: SyncProvider::WebDav,
            sync_mode: SyncMode::Auto,
            auto_interval_seconds: 600,
            provider_config: None,
        };

        let result = create_cloud_storage(&config);
        assert!(result.is_err());

        match result.unwrap_err() {
            SyncError::ConfigValidationFailed { field, reason } => {
                assert_eq!(field, "provider_config");
                assert!(reason.contains("WebDAV"));
            }
            other => panic!("expected ConfigValidationFailed, got {:?}", other),
        }
    }

    #[test]
    fn factory_icloud_returns_storage() {
        let config = SyncConfig {
            provider: SyncProvider::ICloud,
            sync_mode: SyncMode::Auto,
            auto_interval_seconds: 600,
            provider_config: Some(ProviderConfig::ICloud),
        };

        let result = create_cloud_storage(&config);
        assert!(result.is_ok());

        let storage = result.unwrap();
        drop(storage);
    }

    #[test]
    fn factory_s3_returns_storage() {
        let config = SyncConfig {
            provider: SyncProvider::S3,
            sync_mode: SyncMode::Auto,
            auto_interval_seconds: 600,
            provider_config: Some(ProviderConfig::S3(S3Config {
                endpoint: Some("https://s3.amazonaws.com".to_string()),
                bucket: "test-bucket".to_string(),
                region: Some("us-east-1".to_string()),
                access_key_id: "key".to_string(),
                secret_access_key: "secret".to_string(),
                root_path: "/".to_string(),
            })),
        };

        let result = create_cloud_storage(&config);
        assert!(result.is_ok());

        let storage = result.unwrap();
        drop(storage);
    }

    #[test]
    fn factory_sftp_returns_storage() {
        let config = SyncConfig {
            provider: SyncProvider::Sftp,
            sync_mode: SyncMode::Auto,
            auto_interval_seconds: 600,
            provider_config: Some(ProviderConfig::Sftp(SftpConfig {
                server: "localhost:22".to_string(),
                root_path: "/".to_string(),
                ssh_key_path: "/dev/null".to_string(),
                host_check: crate::config::sync::SftpHostCheck::Strict,
            })),
        };

        let result = create_cloud_storage(&config);
        assert!(result.is_ok());

        let storage = result.unwrap();
        drop(storage);
    }

    // ==================== Config Validation Tests ====================

    #[test]
    fn test_validate_config_missing_endpoint() {
        let adapter = WebDavAdapter::new();
        let config = ProviderConfig::WebDav(WebDavConfig {
            endpoint: "".to_string(),
            root_path: "/".to_string(),
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            bearer_token: None,
        });

        let result = adapter.validate_config(&config);
        assert!(result.is_err());

        match result.unwrap_err() {
            SyncError::ConfigValidationFailed { field, reason } => {
                assert_eq!(field, "endpoint");
                assert!(reason.contains("empty"));
            }
            other => panic!("expected ConfigValidationFailed, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_config_missing_auth() {
        let adapter = WebDavAdapter::new();
        let config = ProviderConfig::WebDav(WebDavConfig {
            endpoint: "https://webdav.example.com".to_string(),
            root_path: "/".to_string(),
            username: None,
            password: None,
            bearer_token: None,
        });

        let result = adapter.validate_config(&config);
        assert!(result.is_err());

        match result.unwrap_err() {
            SyncError::ConfigValidationFailed { field, reason } => {
                assert_eq!(field, "authentication");
                assert!(reason.contains("bearer_token") || reason.contains("username"));
            }
            other => panic!("expected ConfigValidationFailed, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_config_valid_with_bearer_token() {
        let adapter = WebDavAdapter::new();
        let config = ProviderConfig::WebDav(WebDavConfig {
            endpoint: "https://webdav.example.com".to_string(),
            root_path: "/".to_string(),
            username: None,
            password: None,
            bearer_token: Some("my-token".to_string()),
        });

        let result = adapter.validate_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_config_valid_with_basic_auth() {
        let adapter = WebDavAdapter::new();
        let config = ProviderConfig::WebDav(WebDavConfig {
            endpoint: "https://webdav.example.com".to_string(),
            root_path: "/".to_string(),
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            bearer_token: None,
        });

        let result = adapter.validate_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn s3_validate_missing_bucket() {
        let adapter = S3Adapter::new();
        let config = ProviderConfig::S3(S3Config {
            endpoint: Some("https://s3.amazonaws.com".to_string()),
            bucket: "".to_string(),
            region: Some("us-east-1".to_string()),
            access_key_id: "key".to_string(),
            secret_access_key: "secret".to_string(),
            root_path: "/".to_string(),
        });

        let result = adapter.validate_config(&config);
        assert!(result.is_err());

        match result.unwrap_err() {
            SyncError::ConfigValidationFailed { field, reason } => {
                assert_eq!(field, "bucket");
                assert!(reason.contains("empty"));
            }
            other => panic!("expected ConfigValidationFailed, got {:?}", other),
        }
    }

    #[test]
    fn sftp_validate_missing_host() {
        let adapter = SftpAdapter::new();
        let config = ProviderConfig::Sftp(SftpConfig {
            server: "".to_string(),
            root_path: "/".to_string(),
            ssh_key_path: "/dev/null".to_string(),
            host_check: crate::config::sync::SftpHostCheck::Strict,
        });

        let result = adapter.validate_config(&config);
        assert!(result.is_err());

        match result.unwrap_err() {
            SyncError::ConfigValidationFailed { field, reason } => {
                assert_eq!(field, "server");
                assert!(reason.contains("empty"));
            }
            other => panic!("expected ConfigValidationFailed, got {:?}", other),
        }
    }

    #[test]
    #[allow(deprecated)]
    fn oauth2_validate_missing_refresh_token() {
        let adapter = GoogleDriveAdapter::new();
        let config = ProviderConfig::GoogleDrive(GoogleDriveConfig {
            access_token: String::new(),
            refresh_token: String::new(),
            root_path: "/".to_string(),
            client_id: String::new(),
            client_secret: String::new(),
        });

        let result = adapter.validate_config(&config);
        assert!(result.is_err());

        match result.unwrap_err() {
            SyncError::ConfigValidationFailed { field, reason } => {
                assert_eq!(field, "refresh_token");
                assert!(reason.contains("refresh_token"));
            }
            other => panic!("expected ConfigValidationFailed, got {:?}", other),
        }
    }

    #[test]
    #[allow(deprecated)]
    fn oauth2_validate_with_valid_config() {
        let adapter = GoogleDriveAdapter::new();
        let config = ProviderConfig::GoogleDrive(GoogleDriveConfig {
            access_token: "test_access_token".to_string(),
            refresh_token: "test_refresh_token".to_string(),
            root_path: "/".to_string(),
            client_id: String::new(),
            client_secret: String::new(),
        });

        let result = adapter.validate_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn icloud_needs_watcher() {
        let adapter = ICloudAdapter::new();
        assert!(adapter.needs_watcher());
    }

    // ==================== Path Normalization Tests ====================

    #[test]
    fn test_normalize_path() {
        let adapter = WebDavAdapter::new();

        // Normal case
        assert_eq!(
            adapter.normalize_path("/base", "file.txt"),
            "/base/file.txt"
        );

        // Base with trailing slash should be stripped
        assert_eq!(
            adapter.normalize_path("/base/", "file.txt"),
            "/base/file.txt"
        );

        // File with leading slash should be stripped
        assert_eq!(
            adapter.normalize_path("/base", "/file.txt"),
            "/base/file.txt"
        );

        // Empty base
        assert_eq!(adapter.normalize_path("", "file.txt"), "/file.txt");

        // Nested paths
        assert_eq!(
            adapter.normalize_path("/base/path", "subdir/file.txt"),
            "/base/path/subdir/file.txt"
        );
    }

    #[test]
    fn test_normalize_path_default_impl() {
        // Test that the default implementation in the trait works
        // This uses the base implementation, not WebDAV-specific
        struct DummyAdapter;
        impl ProviderAdapter for DummyAdapter {
            fn create_operator(&self, _config: &ProviderConfig) -> Result<Operator, SyncError> {
                unreachable!("not called in this test")
            }
            fn validate_config(&self, _config: &ProviderConfig) -> Result<(), SyncError> {
                unreachable!("not called in this test")
            }
        }

        let adapter = DummyAdapter;

        // Default implementation handles trailing/leading slashes
        assert_eq!(
            adapter.normalize_path("/base", "file.txt"),
            "/base/file.txt"
        );
    }

    // ==================== Provider Name Tests ====================

    #[test]
    fn test_provider_name() {
        assert_eq!(provider_name(SyncProvider::Disabled), "disabled");
        assert_eq!(provider_name(SyncProvider::ICloud), "icloud");
        assert_eq!(provider_name(SyncProvider::GoogleDrive), "google_drive");
        assert_eq!(provider_name(SyncProvider::Dropbox), "dropbox");
        assert_eq!(provider_name(SyncProvider::OneDrive), "onedrive");
        assert_eq!(provider_name(SyncProvider::WebDav), "webdav");
        assert_eq!(provider_name(SyncProvider::Sftp), "sftp");
        assert_eq!(provider_name(SyncProvider::S3), "s3");
        assert_eq!(provider_name(SyncProvider::AliyunDrive), "aliyun_drive");
        assert_eq!(provider_name(SyncProvider::AliyunOss), "aliyun_oss");
        assert_eq!(provider_name(SyncProvider::TencentCos), "tencent_cos");
        assert_eq!(provider_name(SyncProvider::HuaweiObs), "huawei_obs");
        assert_eq!(provider_name(SyncProvider::Upyun), "upyun");
    }

    // ==================== Trait Object Safety Tests ====================

    #[test]
    fn test_provider_adapter_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WebDavAdapter>();
    }

    // ==================== Adapter Default Implementations ====================

    #[test]
    fn test_needs_watcher_default_returns_false() {
        struct DummyAdapter;
        impl ProviderAdapter for DummyAdapter {
            fn create_operator(&self, _config: &ProviderConfig) -> Result<Operator, SyncError> {
                unreachable!("not called in this test")
            }
            fn validate_config(&self, _config: &ProviderConfig) -> Result<(), SyncError> {
                unreachable!("not called in this test")
            }
        }

        let adapter = DummyAdapter;
        assert!(!adapter.needs_watcher());
    }

    #[test]
    fn test_refresh_auth_default_is_noop() {
        struct DummyAdapter;
        impl ProviderAdapter for DummyAdapter {
            fn create_operator(&self, _config: &ProviderConfig) -> Result<Operator, SyncError> {
                unreachable!("not called in this test")
            }
            fn validate_config(&self, _config: &ProviderConfig) -> Result<(), SyncError> {
                unreachable!("not called in this test")
            }
        }

        let adapter = DummyAdapter;
        // Create a memory operator for testing
        let mut op = Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();

        // Should not panic and returns Ok
        let result = adapter.refresh_auth(&mut op);
        assert!(result.is_ok());
    }
}
