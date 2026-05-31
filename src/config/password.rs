use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PasswordGenerationStyle {
    #[default]
    Random,
    Memorable,
    Pin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PasswordDefaultsConfig {
    #[serde(default)]
    pub style: PasswordGenerationStyle,
    #[serde(default = "default_length")]
    pub length: usize,
    #[serde(default = "default_true")]
    pub include_lowercase: bool,
    #[serde(default = "default_true")]
    pub include_digits: bool,
    #[serde(default = "default_true")]
    pub include_uppercase: bool,
    #[serde(default = "default_true")]
    pub include_special: bool,
    #[serde(default = "default_memorable_word_count")]
    pub memorable_word_count: usize,
    #[serde(default = "default_true")]
    pub memorable_capitalize: bool,
    #[serde(default = "default_memorable_separator")]
    pub memorable_separator: String,
    #[serde(default = "default_pin_length")]
    pub pin_length: usize,
}

impl Default for PasswordDefaultsConfig {
    fn default() -> Self {
        Self {
            style: PasswordGenerationStyle::Random,
            length: 16,
            include_lowercase: true,
            include_digits: true,
            include_uppercase: true,
            include_special: true,
            memorable_word_count: 4,
            memorable_capitalize: true,
            memorable_separator: "-".to_string(),
            pin_length: 6,
        }
    }
}

fn default_length() -> usize {
    16
}
fn default_memorable_word_count() -> usize {
    4
}
fn default_memorable_separator() -> String {
    "-".to_string()
}
fn default_pin_length() -> usize {
    6
}
fn default_true() -> bool {
    true
}
