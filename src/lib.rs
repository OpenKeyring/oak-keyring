#![allow(clippy::unnecessary_to_owned)]

rust_i18n::i18n!("locales", fallback = "en");

// Re-export t!() macro for integration tests and external usage
pub use rust_i18n::t;

pub mod app;
pub mod cloud;
pub mod commands;
pub mod config;
pub mod crypto;
pub mod db;
pub mod errors;
pub mod executor;
pub mod instance_lock;
pub mod paths;
pub mod security;
pub mod services;
pub mod sync;
pub mod tui;
pub mod types;
