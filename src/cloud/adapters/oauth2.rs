use opendal::Operator;

use crate::cloud::ProviderAdapter;
use crate::config::sync::ProviderConfig;
use crate::errors::mapping::sync::SyncError;

#[derive(Debug, Default)]
pub struct GoogleDriveAdapter;

impl GoogleDriveAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderAdapter for GoogleDriveAdapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            ProviderConfig::GoogleDrive(drive_config) => {
                let mut builder = opendal::services::Gdrive::default()
                    .client_id(&drive_config.client_id)
                    .client_secret(&drive_config.client_secret)
                    .refresh_token(&drive_config.refresh_token);

                if !drive_config.root_path.is_empty() {
                    builder = builder.root(&drive_config.root_path);
                }

                let operator = Operator::new(builder)
                    .map_err(|e| SyncError::ProviderError {
                        provider: "google_drive".to_string(),
                        message: format!("failed to create operator: {}", e),
                    })?
                    .finish();

                Ok(operator)
            }
            _ => Err(SyncError::ProviderError {
                provider: "google_drive".to_string(),
                message: "expected GoogleDrive config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            ProviderConfig::GoogleDrive(drive_config) => {
                if drive_config.client_id.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "client_id".to_string(),
                        reason: "client_id cannot be empty".to_string(),
                    });
                }
                if drive_config.client_secret.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "client_secret".to_string(),
                        reason: "client_secret cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(SyncError::ProviderError {
                provider: "google_drive".to_string(),
                message: "expected GoogleDrive config".to_string(),
            }),
        }
    }

    fn refresh_auth(&self, _operator: &mut Operator) -> Result<(), SyncError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct DropboxAdapter;

impl DropboxAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderAdapter for DropboxAdapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            ProviderConfig::Dropbox(dropbox_config) => {
                let mut builder = opendal::services::Dropbox::default()
                    .client_id(&dropbox_config.client_id)
                    .client_secret(&dropbox_config.client_secret);

                if !dropbox_config.refresh_token.is_empty() {
                    builder = builder.refresh_token(&dropbox_config.refresh_token);
                }

                if !dropbox_config.root_path.is_empty() {
                    builder = builder.root(&dropbox_config.root_path);
                }

                let operator = Operator::new(builder)
                    .map_err(|e| SyncError::ProviderError {
                        provider: "dropbox".to_string(),
                        message: format!("failed to create operator: {}", e),
                    })?
                    .finish();

                Ok(operator)
            }
            _ => Err(SyncError::ProviderError {
                provider: "dropbox".to_string(),
                message: "expected Dropbox config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            ProviderConfig::Dropbox(dropbox_config) => {
                if dropbox_config.client_id.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "client_id".to_string(),
                        reason: "client_id cannot be empty".to_string(),
                    });
                }
                if dropbox_config.client_secret.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "client_secret".to_string(),
                        reason: "client_secret cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(SyncError::ProviderError {
                provider: "dropbox".to_string(),
                message: "expected Dropbox config".to_string(),
            }),
        }
    }

    fn refresh_auth(&self, _operator: &mut Operator) -> Result<(), SyncError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct OneDriveAdapter;

impl OneDriveAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderAdapter for OneDriveAdapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            ProviderConfig::OneDrive(drive_config) => {
                let mut builder = opendal::services::Onedrive::default()
                    .client_id(&drive_config.client_id)
                    .refresh_token(&drive_config.refresh_token);

                if !drive_config.client_secret.is_empty() {
                    builder = builder.client_secret(&drive_config.client_secret);
                }

                if !drive_config.root_path.is_empty() {
                    builder = builder.root(&drive_config.root_path);
                }

                let operator = Operator::new(builder)
                    .map_err(|e| SyncError::ProviderError {
                        provider: "onedrive".to_string(),
                        message: format!("failed to create operator: {}", e),
                    })?
                    .finish();

                Ok(operator)
            }
            _ => Err(SyncError::ProviderError {
                provider: "onedrive".to_string(),
                message: "expected OneDrive config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            ProviderConfig::OneDrive(drive_config) => {
                if drive_config.client_id.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "client_id".to_string(),
                        reason: "client_id cannot be empty".to_string(),
                    });
                }
                if drive_config.client_secret.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "client_secret".to_string(),
                        reason: "client_secret cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(SyncError::ProviderError {
                provider: "onedrive".to_string(),
                message: "expected OneDrive config".to_string(),
            }),
        }
    }

    fn refresh_auth(&self, _operator: &mut Operator) -> Result<(), SyncError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct AliyunDriveAdapter;

impl AliyunDriveAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderAdapter for AliyunDriveAdapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            ProviderConfig::AliyunDrive(drive_config) => {
                let drive_type = match drive_config.drive_type {
                    crate::config::sync::AliyunDriveType::Default => "default",
                    crate::config::sync::AliyunDriveType::Backup => "backup",
                    crate::config::sync::AliyunDriveType::Resource => "resource",
                };

                let mut builder = opendal::services::AliyunDrive::default()
                    .client_id(&drive_config.client_id)
                    .client_secret(&drive_config.client_secret)
                    .refresh_token(&drive_config.refresh_token)
                    .drive_type(drive_type);

                if !drive_config.root_path.is_empty() {
                    builder = builder.root(&drive_config.root_path);
                }

                let operator = Operator::new(builder)
                    .map_err(|e| SyncError::ProviderError {
                        provider: "aliyun_drive".to_string(),
                        message: format!("failed to create operator: {}", e),
                    })?
                    .finish();

                Ok(operator)
            }
            _ => Err(SyncError::ProviderError {
                provider: "aliyun_drive".to_string(),
                message: "expected AliyunDrive config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            ProviderConfig::AliyunDrive(drive_config) => {
                if drive_config.client_id.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "client_id".to_string(),
                        reason: "client_id cannot be empty".to_string(),
                    });
                }
                if drive_config.client_secret.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "client_secret".to_string(),
                        reason: "client_secret cannot be empty".to_string(),
                    });
                }
                if drive_config.refresh_token.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "refresh_token".to_string(),
                        reason: "refresh_token cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(SyncError::ProviderError {
                provider: "aliyun_drive".to_string(),
                message: "expected AliyunDrive config".to_string(),
            }),
        }
    }

    fn refresh_auth(&self, _operator: &mut Operator) -> Result<(), SyncError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::sync::{AliyunDriveType, GoogleDriveConfig, OneDriveConfig};

    fn test_gdrive_config() -> ProviderConfig {
        ProviderConfig::GoogleDrive(GoogleDriveConfig {
            client_id: "test_client_id".to_string(),
            client_secret: "test_client_secret".to_string(),
            refresh_token: "test_refresh_token".to_string(),
            root_path: "/test".to_string(),
        })
    }

    fn test_dropbox_config() -> ProviderConfig {
        ProviderConfig::Dropbox(crate::config::sync::DropboxConfig {
            client_id: "test_client_id".to_string(),
            client_secret: "test_client_secret".to_string(),
            refresh_token: "test_refresh_token".to_string(),
            root_path: "/test".to_string(),
        })
    }

    fn test_onedrive_config() -> ProviderConfig {
        ProviderConfig::OneDrive(OneDriveConfig {
            client_id: "test_client_id".to_string(),
            client_secret: "test_client_secret".to_string(),
            refresh_token: "test_refresh_token".to_string(),
            root_path: "/test".to_string(),
        })
    }

    fn test_aliyun_drive_config() -> ProviderConfig {
        ProviderConfig::AliyunDrive(crate::config::sync::AliyunDriveConfig {
            client_id: "test_client_id".to_string(),
            client_secret: "test_client_secret".to_string(),
            refresh_token: "test_refresh_token".to_string(),
            drive_type: AliyunDriveType::Default,
            root_path: "/test".to_string(),
        })
    }

    #[test]
    fn test_google_drive_adapter_validate_config_valid() {
        let adapter = GoogleDriveAdapter::new();
        let config = test_gdrive_config();
        assert!(adapter.validate_config(&config).is_ok());
    }

    #[test]
    fn test_google_drive_adapter_validate_config_empty_client_id() {
        let adapter = GoogleDriveAdapter::new();
        let mut config = test_gdrive_config();
        if let ProviderConfig::GoogleDrive(ref mut cfg) = config {
            cfg.client_id = "".to_string();
        }
        let result = adapter.validate_config(&config);
        assert!(result.is_err());
        if let Err(SyncError::ConfigValidationFailed { field, .. }) = result {
            assert_eq!(field, "client_id");
        } else {
            panic!("Expected ConfigValidationFailed for client_id");
        }
    }

    #[test]
    fn test_google_drive_adapter_validate_config_empty_client_secret() {
        let adapter = GoogleDriveAdapter::new();
        let mut config = test_gdrive_config();
        if let ProviderConfig::GoogleDrive(ref mut cfg) = config {
            cfg.client_secret = "".to_string();
        }
        let result = adapter.validate_config(&config);
        assert!(result.is_err());
        if let Err(SyncError::ConfigValidationFailed { field, .. }) = result {
            assert_eq!(field, "client_secret");
        } else {
            panic!("Expected ConfigValidationFailed for client_secret");
        }
    }

    #[test]
    fn test_google_drive_adapter_create_operator() {
        let adapter = GoogleDriveAdapter::new();
        let config = test_gdrive_config();
        let result = adapter.create_operator(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_google_drive_adapter_wrong_config() {
        let adapter = GoogleDriveAdapter::new();
        let config = test_dropbox_config();
        let result = adapter.create_operator(&config);
        assert!(result.is_err());
        if let Err(SyncError::ProviderError { provider, .. }) = result {
            assert_eq!(provider, "google_drive");
        } else {
            panic!("Expected ProviderError");
        }
    }

    #[test]
    fn test_dropbox_adapter_validate_config_valid() {
        let adapter = DropboxAdapter::new();
        let config = test_dropbox_config();
        assert!(adapter.validate_config(&config).is_ok());
    }

    #[test]
    fn test_dropbox_adapter_validate_config_empty_client_id() {
        let adapter = DropboxAdapter::new();
        let mut config = test_dropbox_config();
        if let ProviderConfig::Dropbox(ref mut cfg) = config {
            cfg.client_id = "".to_string();
        }
        let result = adapter.validate_config(&config);
        assert!(result.is_err());
        if let Err(SyncError::ConfigValidationFailed { field, .. }) = result {
            assert_eq!(field, "client_id");
        } else {
            panic!("Expected ConfigValidationFailed for client_id");
        }
    }

    #[test]
    fn test_dropbox_adapter_create_operator() {
        let adapter = DropboxAdapter::new();
        let config = test_dropbox_config();
        let result = adapter.create_operator(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_onedrive_adapter_validate_config_valid() {
        let adapter = OneDriveAdapter::new();
        let config = test_onedrive_config();
        assert!(adapter.validate_config(&config).is_ok());
    }

    #[test]
    fn test_onedrive_adapter_validate_config_empty_client_id() {
        let adapter = OneDriveAdapter::new();
        let mut config = test_onedrive_config();
        if let ProviderConfig::OneDrive(ref mut cfg) = config {
            cfg.client_id = "".to_string();
        }
        let result = adapter.validate_config(&config);
        assert!(result.is_err());
        if let Err(SyncError::ConfigValidationFailed { field, .. }) = result {
            assert_eq!(field, "client_id");
        } else {
            panic!("Expected ConfigValidationFailed for client_id");
        }
    }

    #[test]
    fn test_onedrive_adapter_create_operator() {
        let adapter = OneDriveAdapter::new();
        let config = test_onedrive_config();
        let result = adapter.create_operator(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_onedrive_adapter_wrong_config() {
        let adapter = OneDriveAdapter::new();
        let config = test_dropbox_config();
        let result = adapter.create_operator(&config);
        assert!(result.is_err());
        if let Err(SyncError::ProviderError { provider, .. }) = result {
            assert_eq!(provider, "onedrive");
        } else {
            panic!("Expected ProviderError");
        }
    }

    #[test]
    fn test_aliyun_drive_adapter_validate_config_valid() {
        let adapter = AliyunDriveAdapter::new();
        let config = test_aliyun_drive_config();
        assert!(adapter.validate_config(&config).is_ok());
    }

    #[test]
    fn test_aliyun_drive_adapter_validate_config_empty_client_id() {
        let adapter = AliyunDriveAdapter::new();
        let mut config = test_aliyun_drive_config();
        if let ProviderConfig::AliyunDrive(ref mut cfg) = config {
            cfg.client_id = "".to_string();
        }
        let result = adapter.validate_config(&config);
        assert!(result.is_err());
        if let Err(SyncError::ConfigValidationFailed { field, .. }) = result {
            assert_eq!(field, "client_id");
        } else {
            panic!("Expected ConfigValidationFailed for client_id");
        }
    }

    #[test]
    fn test_aliyun_drive_adapter_validate_config_empty_refresh_token() {
        let adapter = AliyunDriveAdapter::new();
        let mut config = test_aliyun_drive_config();
        if let ProviderConfig::AliyunDrive(ref mut cfg) = config {
            cfg.refresh_token = "".to_string();
        }
        let result = adapter.validate_config(&config);
        assert!(result.is_err());
        if let Err(SyncError::ConfigValidationFailed { field, .. }) = result {
            assert_eq!(field, "refresh_token");
        } else {
            panic!("Expected ConfigValidationFailed for refresh_token");
        }
    }

    #[test]
    fn test_aliyun_drive_adapter_create_operator() {
        let adapter = AliyunDriveAdapter::new();
        let config = test_aliyun_drive_config();
        let result = adapter.create_operator(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_aliyun_drive_adapter_drive_type_mapping() {
        let adapter = AliyunDriveAdapter::new();

        for (drive_type, expected_str) in [
            (AliyunDriveType::Default, "default"),
            (AliyunDriveType::Backup, "backup"),
            (AliyunDriveType::Resource, "resource"),
        ] {
            let config = ProviderConfig::AliyunDrive(crate::config::sync::AliyunDriveConfig {
                client_id: "test_client_id".to_string(),
                client_secret: "test_client_secret".to_string(),
                refresh_token: "test_refresh_token".to_string(),
                drive_type,
                root_path: "/test".to_string(),
            });

            let result = adapter.create_operator(&config);
            assert!(
                result.is_ok(),
                "Failed to create operator for drive_type: {:?}",
                drive_type
            );
        }
    }

    #[test]
    fn test_aliyun_drive_adapter_wrong_config() {
        let adapter = AliyunDriveAdapter::new();
        let config = test_dropbox_config();
        let result = adapter.create_operator(&config);
        assert!(result.is_err());
        if let Err(SyncError::ProviderError { provider, .. }) = result {
            assert_eq!(provider, "aliyun_drive");
        } else {
            panic!("Expected ProviderError");
        }
    }
}
