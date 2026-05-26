//! Password generator UI state for U6.

use crate::crypto::password;
use crate::crypto::strength::{PasswordStrength, StrengthLevel};
use crate::types::sensitive::SensitiveInput;
use crate::types::SecureStr;

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
#[derive(Debug)]
pub struct GeneratorState {
    pub style: GenerationStyle,
    pub random_config: RandomConfig,
    pub memorable_config: MemorableConfig,
    pub pin_config: PinConfig,
    /// Generated password preview. Manual `Clone` clears this field so UI
    /// snapshots cannot duplicate generated credentials.
    pub preview: SensitiveInput,
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
            preview: SensitiveInput::new(),
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
            preview: SensitiveInput::new(),
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
                password::generate_random_password_with_char_types(
                    cfg.length,
                    cfg.uppercase,
                    cfg.lowercase,
                    cfg.digits,
                    cfg.symbols,
                )
            }
            GenerationStyle::Memorable => password::generate_memorable_password_with_options(
                self.memorable_config.word_count,
                &self.memorable_config.separator,
                self.memorable_config.capitalize,
            ),
            GenerationStyle::Pin => password::generate_pin(self.pin_config.length),
        };

        match result {
            Ok(pw) => {
                self.strength = Some(if self.style == GenerationStyle::Random {
                    estimate_random_strength(&self.random_config)
                } else {
                    PasswordStrength {
                        level: StrengthLevel::Strong,
                        char_types: 0,
                        bar_fill: 12,
                    }
                });
                self.preview = SensitiveInput::from(pw);
            }
            Err(_) => {
                self.preview = SensitiveInput::new();
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
        let enabled_count = [
            self.random_config.uppercase,
            self.random_config.lowercase,
            self.random_config.digits,
            self.random_config.symbols,
        ]
        .into_iter()
        .filter(|enabled| *enabled)
        .count();
        match index {
            0 if self.random_config.uppercase && enabled_count == 1 => {}
            0 => self.random_config.uppercase = !self.random_config.uppercase,
            1 if self.random_config.lowercase && enabled_count == 1 => {}
            1 => self.random_config.lowercase = !self.random_config.lowercase,
            2 if self.random_config.digits && enabled_count == 1 => {}
            2 => self.random_config.digits = !self.random_config.digits,
            3 if self.random_config.symbols && enabled_count == 1 => {}
            3 => self.random_config.symbols = !self.random_config.symbols,
            _ => {}
        }
        self.regenerate();
    }

    /// Whether a character type toggle is enabled.
    pub fn is_toggle_enabled(&self, index: usize) -> bool {
        match index {
            0 => self.random_config.uppercase,
            1 => self.random_config.lowercase,
            2 => self.random_config.digits,
            3 => self.random_config.symbols,
            _ => false,
        }
    }

    /// Whether a toggle is interactive.
    pub fn is_toggle_interactive(&self, index: usize) -> bool {
        index <= 3
    }

    /// Get tab focus order for current style.
    pub fn focus_order(&self) -> Vec<GeneratorFocus> {
        match self.style {
            GenerationStyle::Random => vec![
                GeneratorFocus::StyleSelector,
                GeneratorFocus::LengthSlider,
                GeneratorFocus::Toggle(0),
                GeneratorFocus::Toggle(1),
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

    pub fn focus_next_toggle(&mut self) {
        let toggles = self.option_focus_order();
        if toggles.is_empty() {
            return;
        }
        if let Some(idx) = toggles.iter().position(|focus| *focus == self.focus) {
            self.focus = toggles[(idx + 1) % toggles.len()];
        }
    }

    pub fn focus_prev_toggle(&mut self) {
        let toggles = self.option_focus_order();
        if toggles.is_empty() {
            return;
        }
        if let Some(idx) = toggles.iter().position(|focus| *focus == self.focus) {
            let len = toggles.len();
            self.focus = toggles[(idx + len - 1) % len];
        }
    }

    fn option_focus_order(&self) -> Vec<GeneratorFocus> {
        match self.style {
            GenerationStyle::Random => vec![
                GeneratorFocus::Toggle(0),
                GeneratorFocus::Toggle(1),
                GeneratorFocus::Toggle(2),
                GeneratorFocus::Toggle(3),
            ],
            GenerationStyle::Memorable => {
                vec![GeneratorFocus::Toggle(0), GeneratorFocus::SeparatorInput]
            }
            GenerationStyle::Pin => Vec::new(),
        }
    }

    /// Move focus to the next section (down arrow).
    /// Sections: StyleSelector → LengthSlider → Options → Buttons → wrap.
    pub fn focus_section_down(&mut self) {
        use GeneratorFocus::*;
        self.focus = match self.focus {
            StyleSelector => LengthSlider,
            LengthSlider => self.first_option_focus(),
            Toggle(_) | SeparatorInput => RegenerateButton,
            RegenerateButton | ActionButton => StyleSelector,
        };
    }

    /// Move focus to the previous section (up arrow).
    pub fn focus_section_up(&mut self) {
        use GeneratorFocus::*;
        self.focus = match self.focus {
            StyleSelector => ActionButton,
            LengthSlider => StyleSelector,
            Toggle(_) | SeparatorInput => LengthSlider,
            RegenerateButton | ActionButton => self.last_option_focus(),
        };
    }

    /// First focusable element in the options section.
    fn first_option_focus(&self) -> GeneratorFocus {
        match self.style {
            GenerationStyle::Random | GenerationStyle::Memorable => {
                GeneratorFocus::Toggle(0)
            }
            GenerationStyle::Pin => GeneratorFocus::RegenerateButton,
        }
    }

    /// Last focusable element in the options section (for up-arrow into options).
    fn last_option_focus(&self) -> GeneratorFocus {
        match self.style {
            GenerationStyle::Random => GeneratorFocus::Toggle(3),
            GenerationStyle::Memorable => GeneratorFocus::SeparatorInput,
            GenerationStyle::Pin => GeneratorFocus::LengthSlider,
        }
    }

    /// Clear preview from memory.
    pub fn clear_preview(&mut self) {
        self.preview.clear();
        self.strength = None;
    }

    pub fn has_preview(&self) -> bool {
        !self.preview.is_empty()
    }

    pub fn preview_expose<R>(&self, f: impl FnOnce(&str) -> R) -> R {
        self.preview.expose(f)
    }

    pub fn take_preview(&mut self) -> SecureStr {
        self.preview.take_secure()
    }
}

fn estimate_random_strength(config: &RandomConfig) -> PasswordStrength {
    let charset_size = [
        (config.uppercase, 24usize),
        (config.lowercase, 24usize),
        (config.digits, 8usize),
        (config.symbols, 12usize),
    ]
    .into_iter()
    .filter_map(|(enabled, size)| enabled.then_some(size))
    .sum::<usize>();

    if charset_size == 0 {
        return PasswordStrength {
            level: StrengthLevel::VeryWeak,
            char_types: 0,
            bar_fill: 3,
        };
    }

    let bits = config.length as f64 * (charset_size as f64).log2();
    let (level, bar_fill) = if bits >= 120.0 {
        (StrengthLevel::VeryStrong, 16)
    } else if bits >= 80.0 {
        (StrengthLevel::Strong, 12)
    } else if bits >= 50.0 {
        (StrengthLevel::Fair, 9)
    } else {
        (StrengthLevel::Weak, 6)
    };

    PasswordStrength {
        level,
        char_types: [
            config.uppercase,
            config.lowercase,
            config.digits,
            config.symbols,
        ]
        .into_iter()
        .filter(|enabled| *enabled)
        .count() as u8,
        bar_fill,
    }
}

/// Embedded generator panel state (inside Create/Edit form).
#[derive(Debug)]
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
    pub fn use_password(&mut self) -> SecureStr {
        let pw = self.generator.take_preview();
        self.expanded = false;
        pw
    }
}

impl Clone for GeneratorState {
    fn clone(&self) -> Self {
        Self {
            style: self.style,
            random_config: self.random_config.clone(),
            memorable_config: self.memorable_config.clone(),
            pin_config: self.pin_config.clone(),
            preview: SensitiveInput::new(),
            strength: None,
            focus: self.focus,
        }
    }
}

impl Clone for EmbeddedGeneratorState {
    fn clone(&self) -> Self {
        Self {
            expanded: self.expanded,
            generator: self.generator.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview(state: &GeneratorState) -> String {
        state.preview_expose(|s| s.to_owned())
    }

    #[test]
    fn new_state_has_random_style() {
        let state = GeneratorState::new();
        assert_eq!(state.style, GenerationStyle::Random);
        assert!(state.has_preview());
        assert!(state.strength.is_some());
    }

    #[test]
    fn regenerate_updates_preview() {
        let mut state = GeneratorState::new();
        state.regenerate();
        assert!(state.has_preview());
    }

    #[test]
    fn set_style_regenerates() {
        let mut state = GeneratorState::new();
        state.set_style(GenerationStyle::Pin);
        assert_eq!(state.style, GenerationStyle::Pin);
        assert!(state.preview_expose(|s| s.chars().all(|c| c.is_ascii_digit())));
        assert_eq!(
            state.strength.as_ref().unwrap().level,
            StrengthLevel::Strong
        );
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
    fn lowercase_toggle_is_interactive() {
        let mut state = GeneratorState::new();
        assert!(state.is_toggle_enabled(1));
        assert!(state.is_toggle_interactive(1));
        state.toggle_char_type(1);
        assert!(!state.random_config.lowercase);
    }

    #[test]
    fn random_generation_respects_disabled_character_types() {
        let mut state = GeneratorState::new();
        state.random_config.length = 32;
        state.random_config.uppercase = false;
        state.random_config.lowercase = false;
        state.random_config.symbols = false;
        state.random_config.digits = true;
        state.regenerate();

        assert!(state.preview_expose(|s| s.chars().all(|c| c.is_ascii_digit())));
    }

    #[test]
    fn random_strength_uses_entropy_when_character_types_are_disabled() {
        let mut state = GeneratorState::new();
        state.random_config.length = 24;
        state.random_config.uppercase = true;
        state.random_config.lowercase = true;
        state.random_config.digits = true;
        state.random_config.symbols = false;
        state.regenerate();

        assert_eq!(
            state.strength.as_ref().unwrap().level,
            StrengthLevel::VeryStrong
        );
    }

    #[test]
    fn random_generation_keeps_last_character_type_enabled() {
        let mut state = GeneratorState::new();
        state.random_config.uppercase = false;
        state.random_config.lowercase = true;
        state.random_config.digits = false;
        state.random_config.symbols = false;
        state.toggle_char_type(1);

        assert!(state.random_config.lowercase);
    }

    #[test]
    fn focus_order_random_has_8_items() {
        let state = GeneratorState::new();
        assert_eq!(state.focus_order().len(), 8);
    }

    #[test]
    fn memorable_option_focus_moves_between_capitalize_and_separator() {
        let mut state = GeneratorState::new();
        state.set_style(GenerationStyle::Memorable);
        state.focus = GeneratorFocus::Toggle(0);

        state.focus_next_toggle();
        assert_eq!(state.focus, GeneratorFocus::SeparatorInput);

        state.focus_prev_toggle();
        assert_eq!(state.focus, GeneratorFocus::Toggle(0));
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
        assert!(state.generator.has_preview());
        state.collapse();
        assert!(!state.expanded);
        assert!(!state.generator.has_preview());
    }

    #[test]
    fn embedded_use_password_returns_and_collapses() {
        let mut state = EmbeddedGeneratorState::new();
        state.expand();
        let pw = state.use_password();
        assert!(!pw.expose().is_empty());
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

    #[test]
    fn memorable_capitalize_true_produces_capitalized_words() {
        let mut state = GeneratorState::new();
        state.style = GenerationStyle::Memorable;
        state.memorable_config.capitalize = true;
        state.memorable_config.word_count = 4;
        state.memorable_config.separator = "-".to_string();
        state.regenerate();

        // Split by separator and check each word starts with uppercase
        let preview = preview(&state);
        let words: Vec<&str> = preview.split('-').collect();
        assert_eq!(words.len(), 4, "Should have 4 words separated by '-'");

        for word in words {
            assert!(!word.is_empty(), "Word should not be empty");
            let first_char = word.chars().next().unwrap();
            assert!(
                first_char.is_uppercase(),
                "First character of '{}' should be uppercase when capitalize=true",
                word
            );
        }
    }

    #[test]
    fn memorable_capitalize_false_produces_lowercase_words() {
        let mut state = GeneratorState::new();
        state.style = GenerationStyle::Memorable;
        state.memorable_config.capitalize = false;
        state.memorable_config.word_count = 3;
        state.memorable_config.separator = "-".to_string();
        state.regenerate();

        // Split by separator and check each word is all lowercase
        let preview = preview(&state);
        let words: Vec<&str> = preview.split('-').collect();
        assert_eq!(words.len(), 3, "Should have 3 words separated by '-'");

        for word in words {
            assert!(!word.is_empty(), "Word should not be empty");
            assert!(
                word.chars()
                    .all(|c| c.is_lowercase() || c.is_ascii_lowercase()),
                "Word '{}' should be all lowercase when capitalize=false",
                word
            );
        }
    }

    #[test]
    fn memorable_default_config_has_capitalize_true() {
        let state = GeneratorState::new();
        assert!(state.memorable_config.capitalize);
    }

    #[test]
    fn memorable_regenerate_respects_capitalize_toggle() {
        let mut state = GeneratorState::new();
        state.style = GenerationStyle::Memorable;
        state.memorable_config.capitalize = true;
        state.regenerate();

        // With capitalize=true, preview should have uppercase letters
        let has_uppercase = state.preview_expose(|s| s.chars().any(|c| c.is_uppercase()));
        assert!(
            has_uppercase,
            "Preview should contain uppercase letters when capitalize=true"
        );

        // Toggle to false and regenerate
        state.memorable_config.capitalize = false;
        state.regenerate();

        // With capitalize=false, preview should be all lowercase (except separator)
        let preview = preview(&state);
        let parts: Vec<&str> = preview.split('-').collect();
        for part in parts {
            assert!(
                part.chars().all(|c| c.is_lowercase()),
                "Word '{}' should be lowercase when capitalize=false",
                part
            );
        }
    }
}
