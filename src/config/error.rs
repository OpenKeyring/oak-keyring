use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext, ErrorLevel};

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
    fn error_code(&self) -> ErrorCode {
        match self {
            ConfigError::Io(e) => ErrorCode::Config(format!("IO: {e}")),
            ConfigError::Parse(msg) => ErrorCode::Config(format!("PARSE: {msg}")),
            ConfigError::Validation(msg) => ErrorCode::Config(format!("VALIDATION: {msg}")),
        }
    }

    fn error_context(&self) -> Option<ErrorContext> {
        None
    }

    fn error_level(&self) -> ErrorLevel {
        match self {
            ConfigError::Io(_) => ErrorLevel::Error,
            ConfigError::Parse(_) => ErrorLevel::Error,
            ConfigError::Validation(_) => ErrorLevel::Warning,
        }
    }
}
