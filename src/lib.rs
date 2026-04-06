rust_i18n::i18n!("locales", fallback = "en");

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
