use opendal::Operator;
use std::path::PathBuf;

use crate::cloud::ProviderAdapter;
use crate::config::sync::ProviderConfig;
use crate::errors::mapping::sync::SyncError;

#[derive(Debug, Default)]
pub struct ICloudAdapter;

impl ICloudAdapter {
    pub fn new() -> Self {
        Self
    }

    fn icloud_path() -> PathBuf {
        crate::paths::document_dir()
            .join("..")
            .join("Library")
            .join("Mobile Documents")
    }
}

impl ProviderAdapter for ICloudAdapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            ProviderConfig::ICloud => {
                let icloud_path = Self::icloud_path();
                let builder = opendal::services::Fs::default().root(&icloud_path.to_string_lossy());

                let operator = Operator::new(builder)
                    .map_err(|e| SyncError::ProviderError {
                        provider: "icloud".to_string(),
                        message: format!("failed to create operator: {}", e),
                    })?
                    .finish();

                Ok(operator)
            }
            _ => Err(SyncError::ProviderError {
                provider: "icloud".to_string(),
                message: "expected ICloud config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            ProviderConfig::ICloud => Ok(()),
            _ => Err(SyncError::ProviderError {
                provider: "icloud".to_string(),
                message: "expected ICloud config".to_string(),
            }),
        }
    }

    fn needs_watcher(&self) -> bool {
        true
    }

    fn test_connection(&self, operator: &Operator) -> Result<(), SyncError> {
        let _ = operator;
        Ok(())
    }
}
