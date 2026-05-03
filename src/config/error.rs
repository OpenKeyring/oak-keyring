use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("config parse error: {0}")]
    Parse(String),

    #[error("config validation error: {0}")]
    Validation(String),
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Parse(e.to_string())
    }
}

impl From<toml::ser::Error> for ConfigError {
    fn from(e: toml::ser::Error) -> Self {
        ConfigError::Parse(e.to_string())
    }
}

impl ServiceError for ConfigError {
    fn to_error_code(&self) -> ErrorCode {
        match self {
            ConfigError::Io(_) => ErrorCode::VaultDatabaseIoError,
            ConfigError::Parse(_) => ErrorCode::ImportFileFormatInvalid,
            ConfigError::Validation(_) => ErrorCode::ImportColumnMappingInvalid,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        ErrorContext::new()
    }

    fn to_fallback_message(&self) -> String {
        self.to_string()
    }
}
