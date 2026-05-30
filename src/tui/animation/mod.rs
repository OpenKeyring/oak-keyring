pub mod effects;
pub mod transitions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationLevel {
    #[default]
    Full, // True color + 30fps+
    Reduced, // Character-level only
    None,    // Instant transitions
}

pub fn level_for_mode(mode: crate::config::AnimationMode) -> AnimationLevel {
    match mode {
        crate::config::AnimationMode::On => AnimationLevel::Full,
        crate::config::AnimationMode::Off => AnimationLevel::None,
        crate::config::AnimationMode::Auto => detect_animation_level(),
    }
}

/// Detect animation capability from terminal environment.
pub fn detect_animation_level() -> AnimationLevel {
    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();

    // Terminals known to support true color.
    let true_color_terminals = ["iTerm.app", "ghostty", "Hyper", "WezTerm", "vscode"];

    if colorterm == "truecolor"
        || colorterm == "24bit"
        || true_color_terminals.iter().any(|&t| term_program == t)
    {
        AnimationLevel::Full
    } else {
        // Assume at least reduced for any 256-color terminal.
        AnimationLevel::Reduced
    }
}
