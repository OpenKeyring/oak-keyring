use opendal::Operator;

use crate::cloud::credentials::{google_client_id, google_client_secret};
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
                let built_in_client_id = google_client_id();
                let built_in_client_secret = google_client_secret();

                let mut builder = opendal::services::Gdrive::default()
                    .client_id(&built_in_client_id)
                    .client_secret(&built_in_client_secret);

                if !drive_config.refresh_token.is_empty() {
                    builder = builder.refresh_token(&drive_config.refresh_token);
                }
                if !drive_config.access_token.is_empty() {
                    builder = builder.access_token(&drive_config.access_token);
                }
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
                if drive_config.refresh_token.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "refresh_token".to_string(),
                        reason: "refresh_token 不能为空，请先完成授权".to_string(),
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
        // opendal handles token refresh internally
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
            ProviderConfig::Dropbox(_) => Err(SyncError::ProviderNotSupported {
                provider: "dropbox".to_string(),
            }),
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
            ProviderConfig::OneDrive(_) => Err(SyncError::ProviderNotSupported {
                provider: "onedrive".to_string(),
            }),
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
            ProviderConfig::AliyunDrive(_) => Err(SyncError::ProviderNotSupported {
                provider: "aliyun_drive".to_string(),
            }),
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
