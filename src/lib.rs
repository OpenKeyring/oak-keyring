#![allow(clippy::unnecessary_to_owned)]

rust_i18n::i18n!("locales", fallback = "en");

// `t!` is the per-thread-locale wrapper defined in `tui::i18n`. It is
// `#[macro_export]`-ed to the crate root, so callers reach it via `crate::t`
// (in-tree) and `oak_keyring::t!` (integration tests / external usage).

pub mod app;
pub mod cloud;
pub mod commands;
pub mod config;
pub mod crypto;
pub mod db;
pub mod errors;
pub mod executor;
pub mod instance_lock;
pub mod logging;
pub mod paths;
pub mod security;
pub mod services;
pub mod sync;
pub mod tui;
pub mod types;
