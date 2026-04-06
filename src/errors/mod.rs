pub mod code;
pub mod context;
pub mod level;
pub mod mapping;
pub mod service_error;

pub use code::ErrorCode;
pub use context::ErrorContext;
pub use level::ErrorLevel;
pub use service_error::ServiceErrorBox;

pub type Result<T> = std::result::Result<T, ServiceErrorBox>;
