//! Password generator UI state for U6.

use crate::crypto::password;
use crate::crypto::strength::{evaluate_strength, PasswordStrength};

/// Current generation style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationStyle {
    Random,
    Memorable,
    Pin,
}

/// Random style configuration.
#[derive(Debug, Clone)]
pub struct RandomConfig {
    pub length: usize,
    pub uppercase: bool,
    pub lowercase: bool,
    pub digits: bool,
    pub symbols: bool,
}

impl Default for RandomConfig {
    fn default() -> Self {
        Self {
            length: 16,
            uppercase: true,
            lowercase: true,
            digits: true,
            symbols: true,
        }
    }
}

/// Memorable style configuration.
#[derive(Debug, Clone)]
pub struct MemorableConfig {
    pub word_count: usize,
    pub capitalize: bool,
    pub separator: String,
}

impl Default for MemorableConfig {
    fn default() -> Self {
        Self {
            word_count: 4,
            capitalize: true,
            separator: "-".to_string(),
        }
    }
}

/// PIN style configuration.
#[derive(Debug, Clone)]
pub struct PinConfig {
    pub length: usize,
}

impl Default for PinConfig {
    fn default() -> Self {
        Self { length: 6 }
    }
}

/// Focus position within the generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratorFocus {
    StyleSelector,
    LengthSlider,
    Toggle(usize),
    SeparatorInput,
    RegenerateButton,
    ActionButton,
}

/// Shared generator state for both standalone dialog and embedded panel.
#[derive(Debug, Clone)]
pub struct GeneratorState {
    pub style: GenerationStyle,
    pub random_config: RandomConfig,
    pub memorable_config: MemorableConfig,
    pub pin_config: PinConfig,
    pub preview: String,
    pub strength: Option<PasswordStrength>,
    pub focus: GeneratorFocus,
}

impl Default for GeneratorState {
    fn default() -> Self {
        Self::new()
    }
}

impl GeneratorState {
    /// Create a new state with default Random config.
    pub fn new() -> Self {
        let mut state = Self {
            style: GenerationStyle::Random,
            random_config: RandomConfig::default(),
            memorable_config: MemorableConfig::default(),
            pin_config: PinConfig::default(),
            preview: String::new(),
            strength: None,
            focus: GeneratorFocus::StyleSelector,
        };
        state.regenerate();
        state
    }

    /// Create a new state from D3 GeneratorConfig defaults.
    /// Falls back to hardcoded defaults if config fields are None.
    pub fn from_config(
        default_length: Option<usize>,
        default_uppercase: Option<bool>,
        default_digits: Option<bool>,
        default_symbols: Option<bool>,
    ) -> Self {
        let mut random_config = RandomConfig::default();
        if let Some(len) = default_length {
            random_config.length = len.clamp(8, 128);
        }
        if let Some(upper) = default_uppercase {
            random_config.uppercase = upper;
        }
        if let Some(digits) = default_digits {
            random_config.digits = digits;
        }
        if let Some(symbols) = default_symbols {
            random_config.symbols = symbols;
        }

        let mut state = Self {
            style: GenerationStyle::Random,
            random_config,
            memorable_config: MemorableConfig::default(),
            pin_config: PinConfig::default(),
            preview: String::new(),
            strength: None,
            focus: GeneratorFocus::StyleSelector,
        };
        state.regenerate();
        state
    }

    /// Regenerate password based on current config.
    pub fn regenerate(&mut self) {
        let result = match self.style {
            GenerationStyle::Random => {
                let cfg = &self.random_config;
                password::generate_random_password_with_policy(
                    cfg.length,
                    if cfg.digits { 1 } else { 0 },
                    if cfg.symbols { 1 } else { 0 },
                    if cfg.lowercase { 1 } else { 0 },
                    if cfg.uppercase { 1 } else { 0 },
                )
            }
            GenerationStyle::Memorable => password::generate_memorable_password_with_separator(
                self.memorable_config.word_count,
                &self.memorable_config.separator,
            ),
            GenerationStyle::Pin => password::generate_pin(self.pin_config.length),
        };

        match result {
            Ok(pw) => {
                let pw_str = pw.get().to_string();
                self.strength = Some(evaluate_strength(&pw_str));
                self.preview = pw_str;
            }
            Err(_) => {
                self.preview = String::new();
                self.strength = None;
            }
        }
    }

    /// Switch generation style and regenerate.
    pub fn set_style(&mut self, style: GenerationStyle) {
        self.style = style;
        self.focus = GeneratorFocus::StyleSelector;
        self.regenerate();
    }

    /// Get current length value for display.
    pub fn current_length(&self) -> usize {
        match self.style {
            GenerationStyle::Random => self.random_config.length,
            GenerationStyle::Memorable => self.memorable_config.word_count,
            GenerationStyle::Pin => self.pin_config.length,
        }
    }

    /// Get min/max for current style's length slider.
    pub fn length_range(&self) -> (usize, usize) {
        match self.style {
            GenerationStyle::Random => (8, 128),
            GenerationStyle::Memorable => (3, 12),
            GenerationStyle::Pin => (4, 16),
        }
    }

    /// Increment length (clamp to max).
    pub fn increment_length(&mut self) {
        let (_, max) = self.length_range();
        match self.style {
            GenerationStyle::Random => {
                self.random_config.length = (self.random_config.length + 1).min(max);
            }
            GenerationStyle::Memorable => {
                self.memorable_config.word_count = (self.memorable_config.word_count + 1).min(max);
            }
            GenerationStyle::Pin => {
                self.pin_config.length = (self.pin_config.length + 1).min(max);
            }
        }
        self.regenerate();
    }

    /// Decrement length (clamp to min).
    pub fn decrement_length(&mut self) {
        let (min, _) = self.length_range();
        match self.style {
            GenerationStyle::Random => {
                self.random_config.length = self.random_config.length.saturating_sub(1).max(min);
            }
            GenerationStyle::Memorable => {
                self.memorable_config.word_count =
                    self.memorable_config.word_count.saturating_sub(1).max(min);
            }
            GenerationStyle::Pin => {
                self.pin_config.length = self.pin_config.length.saturating_sub(1).max(min);
            }
        }
        self.regenerate();
    }

    /// Toggle a character type (only for Random style).
    pub fn toggle_char_type(&mut self, index: usize) {
        if self.style != GenerationStyle::Random {
            return;
        }
        match index {
            0 => self.random_config.uppercase = !self.random_config.uppercase,
            1 => { /* lowercase always on, no-op */ }
            2 => self.random_config.digits = !self.random_config.digits,
            3 => self.random_config.symbols = !self.random_config.symbols,
            _ => {}
        }
        self.regenerate();
    }

    /// Whether a character type toggle is enabled (lowercase is always on).
    pub fn is_toggle_enabled(&self, index: usize) -> bool {
        match index {
            0 => self.random_config.uppercase,
            1 => true,
            2 => self.random_config.digits,
            3 => self.random_config.symbols,
            _ => false,
        }
    }

    /// Whether a toggle is interactive (lowercase is not).
    pub fn is_toggle_interactive(&self, index: usize) -> bool {
        index != 1
    }

    /// Get tab focus order for current style.
    pub fn focus_order(&self) -> Vec<GeneratorFocus> {
        match self.style {
            GenerationStyle::Random => vec![
                GeneratorFocus::StyleSelector,
                GeneratorFocus::LengthSlider,
                GeneratorFocus::Toggle(0),
                GeneratorFocus::Toggle(2),
                GeneratorFocus::Toggle(3),
                GeneratorFocus::RegenerateButton,
                GeneratorFocus::ActionButton,
            ],
            GenerationStyle::Memorable => vec![
                GeneratorFocus::StyleSelector,
                GeneratorFocus::LengthSlider,
                GeneratorFocus::Toggle(0),
                GeneratorFocus::SeparatorInput,
                GeneratorFocus::RegenerateButton,
                GeneratorFocus::ActionButton,
            ],
            GenerationStyle::Pin => vec![
                GeneratorFocus::StyleSelector,
                GeneratorFocus::LengthSlider,
                GeneratorFocus::RegenerateButton,
                GeneratorFocus::ActionButton,
            ],
        }
    }

    /// Advance focus to next in tab order.
    pub fn focus_next(&mut self) {
        let order = self.focus_order();
        if let Some(idx) = order.iter().position(|f| *f == self.focus) {
            self.focus = order[(idx + 1) % order.len()];
        }
    }

    /// Move focus to previous in tab order.
    pub fn focus_prev(&mut self) {
        let order = self.focus_order();
        if let Some(idx) = order.iter().position(|f| *f == self.focus) {
            let len = order.len();
            self.focus = order[(idx + len - 1) % len];
        }
    }

    /// Clear preview from memory.
    pub fn clear_preview(&mut self) {
        let _ = std::mem::take(&mut self.preview);
        self.strength = None;
    }
}

/// Embedded generator panel state (inside Create/Edit form).
#[derive(Debug, Clone)]
pub struct EmbeddedGeneratorState {
    pub expanded: bool,
    pub generator: GeneratorState,
}

impl Default for EmbeddedGeneratorState {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddedGeneratorState {
    pub fn new() -> Self {
        Self {
            expanded: false,
            generator: GeneratorState::new(),
        }
    }

    /// Expand the panel.
    pub fn expand(&mut self) {
        self.expanded = true;
        self.generator = GeneratorState::new();
        self.generator.focus = GeneratorFocus::LengthSlider;
    }

    /// Collapse without filling.
    pub fn collapse(&mut self) {
        self.generator.clear_preview();
        self.expanded = false;
    }

    /// Use the generated password.
    pub fn use_password(&mut self) -> String {
        let pw = std::mem::take(&mut self.generator.preview);
        self.expanded = false;
        pw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_has_random_style() {
        let state = GeneratorState::new();
        assert_eq!(state.style, GenerationStyle::Random);
        assert!(!state.preview.is_empty());
        assert!(state.strength.is_some());
    }

    #[test]
    fn regenerate_updates_preview() {
        let mut state = GeneratorState::new();
        state.regenerate();
        assert!(!state.preview.is_empty());
    }

    #[test]
    fn set_style_regenerates() {
        let mut state = GeneratorState::new();
        state.set_style(GenerationStyle::Pin);
        assert_eq!(state.style, GenerationStyle::Pin);
        assert!(state.preview.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn increment_length_clamps_to_max() {
        let mut state = GeneratorState::new();
        state.random_config.length = 128;
        state.increment_length();
        assert_eq!(state.random_config.length, 128);
    }

    #[test]
    fn decrement_length_clamps_to_min() {
        let mut state = GeneratorState::new();
        state.random_config.length = 8;
        state.decrement_length();
        assert_eq!(state.random_config.length, 8);
    }

    #[test]
    fn toggle_uppercase() {
        let mut state = GeneratorState::new();
        assert!(state.random_config.uppercase);
        state.toggle_char_type(0);
        assert!(!state.random_config.uppercase);
    }

    #[test]
    fn lowercase_always_on() {
        let state = GeneratorState::new();
        assert!(state.is_toggle_enabled(1));
        assert!(!state.is_toggle_interactive(1));
    }

    #[test]
    fn focus_order_random_has_7_items() {
        let state = GeneratorState::new();
        assert_eq!(state.focus_order().len(), 7);
    }

    #[test]
    fn focus_order_pin_has_4_items() {
        let mut state = GeneratorState::new();
        state.set_style(GenerationStyle::Pin);
        assert_eq!(state.focus_order().len(), 4);
    }

    #[test]
    fn focus_next_cycles() {
        let mut state = GeneratorState::new();
        state.focus = GeneratorFocus::ActionButton;
        state.focus_next();
        assert_eq!(state.focus, GeneratorFocus::StyleSelector);
    }

    #[test]
    fn embedded_expand_sets_focus_to_slider() {
        let mut state = EmbeddedGeneratorState::new();
        state.expand();
        assert!(state.expanded);
        assert_eq!(state.generator.focus, GeneratorFocus::LengthSlider);
    }

    #[test]
    fn embedded_collapse_clears_preview() {
        let mut state = EmbeddedGeneratorState::new();
        state.expand();
        assert!(!state.generator.preview.is_empty());
        state.collapse();
        assert!(!state.expanded);
        assert!(state.generator.preview.is_empty());
    }

    #[test]
    fn embedded_use_password_returns_and_collapses() {
        let mut state = EmbeddedGeneratorState::new();
        state.expand();
        let pw = state.use_password();
        assert!(!pw.is_empty());
        assert!(!state.expanded);
    }

    #[test]
    fn from_config_applies_custom_length() {
        let state = GeneratorState::from_config(Some(20), None, None, None);
        assert_eq!(state.random_config.length, 20);
    }

    #[test]
    fn from_config_clamps_invalid_length() {
        let state = GeneratorState::from_config(Some(200), None, None, None);
        assert_eq!(state.random_config.length, 128);
    }

    #[test]
    fn from_config_disables_symbols() {
        let state = GeneratorState::from_config(None, None, None, Some(false));
        assert!(!state.random_config.symbols);
    }
}
