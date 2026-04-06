use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordDefaultsConfig {
    #[serde(default = "default_length")]
    pub length: usize,
    #[serde(default = "default_true")]
    pub include_digits: bool,
    #[serde(default = "default_true")]
    pub include_uppercase: bool,
    #[serde(default = "default_true")]
    pub include_special: bool,
}

impl Default for PasswordDefaultsConfig {
    fn default() -> Self {
        Self {
            length: 16,
            include_digits: true,
            include_uppercase: true,
            include_special: true,
        }
    }
}

fn default_length() -> usize {
    16
}
fn default_true() -> bool {
    true
}
