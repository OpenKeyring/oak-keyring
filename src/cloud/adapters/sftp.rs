use opendal::Operator;

use crate::cloud::ProviderAdapter;
use crate::config::sync::ProviderConfig;
use crate::errors::mapping::sync::SyncError;

#[derive(Debug, Default)]
pub struct SftpAdapter;

impl SftpAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderAdapter for SftpAdapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            crate::config::sync::ProviderConfig::Sftp(sftp_config) => {
                let endpoint = if let Some((host, port_str)) = sftp_config.server.rsplit_once(':') {
                    if port_str.parse::<u16>().is_ok() {
                        sftp_config.server.clone()
                    } else {
                        format!("{}:22", sftp_config.server)
                    }
                } else {
                    format!("{}:22", sftp_config.server)
                };

                let mut builder = opendal::services::Sftp::default()
                    .endpoint(&endpoint)
                    .user("sftp")
                    .key(&sftp_config.ssh_key_path);

                if !sftp_config.root_path.is_empty() {
                    builder = builder.root(&sftp_config.root_path);
                }

                let operator = Operator::new(builder)
                    .map_err(|e| SyncError::ProviderError {
                        provider: "sftp".to_string(),
                        message: format!("failed to create operator: {}", e),
                    })?
                    .finish();

                Ok(operator)
            }
            _ => Err(SyncError::ProviderError {
                provider: "sftp".to_string(),
                message: "expected Sftp config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            crate::config::sync::ProviderConfig::Sftp(sftp_config) => {
                if sftp_config.server.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "server".to_string(),
                        reason: "server cannot be empty".to_string(),
                    });
                }
                if sftp_config.ssh_key_path.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "ssh_key_path".to_string(),
                        reason: "ssh_key_path cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(SyncError::ProviderError {
                provider: "sftp".to_string(),
                message: "expected Sftp config".to_string(),
            }),
        }
    }
}
