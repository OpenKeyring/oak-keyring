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
            // Io defaults to ConfigLoadFailed (executor layer will override for save context)
            ConfigError::Io(_) => ErrorCode::ConfigLoadFailed,
            ConfigError::Parse(_) => ErrorCode::ConfigLoadFailed,
            ConfigError::Validation(_) => ErrorCode::ConfigValidationFailed,
        }
    }

    fn to_error_context(&self) -> ErrorContext {
        // No context for config errors
        ErrorContext::new()
    }

    fn to_fallback_message(&self) -> String {
        self.to_string()
    }
}
