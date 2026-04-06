pub trait ServiceError: std::error::Error + Send + Sync + 'static {
    fn error_code(&self) -> crate::errors::ErrorCode;
    fn error_context(&self) -> Option<crate::errors::ErrorContext>;
    fn error_level(&self) -> crate::errors::ErrorLevel;
}

pub type ServiceErrorBox = Box<dyn ServiceError>;
