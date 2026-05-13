use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AnimationMode {
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralConfig {
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
            auto_lock_seconds: default_auto_lock(),
            clipboard_clear_seconds: default_clipboard(),
            trash_retention_days: default_trash(),
            animation: AnimationMode::Auto,
            language: default_language(),
        }
    }
}
