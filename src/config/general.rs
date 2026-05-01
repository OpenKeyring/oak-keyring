use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AnimationMode {
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_vault_path")]
    pub vault_path: PathBuf,
    #[serde(default = "default_auto_lock")]
    pub auto_lock_seconds: u64,
    #[serde(default = "default_clipboard")]
    pub clipboard_clear_seconds: u64,
    #[serde(default = "default_trash")]
    pub trash_retention_days: u32,
    #[serde(default)]
    pub animation: AnimationMode,
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_vault_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("open-keyring")
}

/// Returns the default vault path as a PathBuf for filesystem operations.
pub fn default_vault_pathbuf() -> PathBuf {
    default_vault_path()
}

/// Returns the default vault path as a user-facing display string.
/// Replaces the home directory prefix with `~` for compact display.
pub fn default_vault_path_display() -> String {
    let path = default_vault_path();
    if let Some(home) = dirs::home_dir() {
        let path_str = path.to_string_lossy();
        let home_str = home.to_string_lossy();
        if let Some(rest) = path_str.strip_prefix(home_str.as_ref()) {
            return format!("~{}", rest);
        }
    }
    path.to_string_lossy().to_string()
}
fn default_auto_lock() -> u64 {
    300
}
fn default_clipboard() -> u64 {
    30
}
fn default_trash() -> u32 {
    30
}
fn default_language() -> String {
    "auto".into()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            vault_path: default_vault_path(),
            auto_lock_seconds: default_auto_lock(),
            clipboard_clear_seconds: default_clipboard(),
            trash_retention_days: default_trash(),
            animation: AnimationMode::Auto,
            language: default_language(),
        }
    }
}
