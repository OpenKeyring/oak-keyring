rust_i18n::i18n!("locales", fallback = "en");

// Re-export t!() macro for integration tests and external usage
pub use rust_i18n::t;

pub mod app;
pub mod commands;
pub mod config;
pub mod crypto;
pub mod db;
pub mod errors;
pub mod executor;
pub mod services;
pub mod tui;
pub mod types;
