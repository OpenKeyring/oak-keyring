pub mod effects;
pub mod transitions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationLevel {
    #[default]
    Full,     // True color + 30fps+
    Reduced,  // Character-level only
    None,     // Instant transitions
}

/// Detect animation capability from terminal environment.
pub fn detect_animation_level() -> AnimationLevel {
    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    let term = std::env::var("TERM").unwrap_or_default();

    if colorterm == "truecolor" || colorterm == "24bit" {
        AnimationLevel::Full
    } else if term.contains("256color") || term.contains("xterm") {
        AnimationLevel::Reduced
    } else {
        AnimationLevel::None
    }
}
