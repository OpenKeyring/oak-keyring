//! Terminal utilities: size detection, responsive breakpoints, OSC 0 title.
//!
//! Provides helper types and functions for adapting the TUI layout to
//! varying terminal sizes and for setting/clearing the window title.

use std::io::Write;

/// Minimum supported terminal size (columns).
pub const MIN_WIDTH: u16 = 80;
/// Minimum supported terminal size (rows).
pub const MIN_HEIGHT: u16 = 24;

/// Responsive breakpoint thresholds (width in columns).
pub const BREAKPOINT_FULL: u16 = 120;
pub const BREAKPOINT_MEDIUM: u16 = 100;
pub const BREAKPOINT_MINIMUM: u16 = 80;

/// Terminal width tier for responsive layout decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthTier {
    /// >= 120 columns: full sidebar, spacious layout.
    Full,
    /// 100-119 columns: narrower sidebar.
    Medium,
    /// 80-99 columns: minimal sidebar.
    Minimum,
    /// < 80 columns: too small for normal layout.
    TooSmall,
}

impl WidthTier {
    /// Determine the width tier from a column count.
    pub fn from_width(w: u16) -> Self {
        if w < MIN_WIDTH {
            Self::TooSmall
        } else if w < BREAKPOINT_MEDIUM {
            Self::Minimum
        } else if w < BREAKPOINT_FULL {
            Self::Medium
        } else {
            Self::Full
        }
    }
}

/// Recommended sidebar width based on the terminal width tier.
pub fn sidebar_width(tier: WidthTier) -> u16 {
    match tier {
        WidthTier::Full => 50,
        WidthTier::Medium => 40,
        WidthTier::Minimum => 30,
        WidthTier::TooSmall => 0,
    }
}

/// Set the terminal window title via OSC 0 escape sequence.
///
/// Errors are silently ignored (e.g. when stdout is not a terminal).
pub fn set_terminal_title(title: &str) {
    let _ = std::io::stdout().write_all(format!("\x1b]0;{}\x07", title).as_bytes());
    let _ = std::io::stdout().flush();
}

/// Clear the terminal window title on exit.
///
/// Errors are silently ignored.
pub fn clear_terminal_title() {
    let _ = std::io::stdout().write_all(b"\x1b]0;\x07");
    let _ = std::io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_tier_from_width() {
        assert_eq!(WidthTier::from_width(60), WidthTier::TooSmall);
        assert_eq!(WidthTier::from_width(79), WidthTier::TooSmall);
        assert_eq!(WidthTier::from_width(80), WidthTier::Minimum);
        assert_eq!(WidthTier::from_width(99), WidthTier::Minimum);
        assert_eq!(WidthTier::from_width(100), WidthTier::Medium);
        assert_eq!(WidthTier::from_width(119), WidthTier::Medium);
        assert_eq!(WidthTier::from_width(120), WidthTier::Full);
        assert_eq!(WidthTier::from_width(200), WidthTier::Full);
    }

    #[test]
    fn sidebar_width_values() {
        assert_eq!(sidebar_width(WidthTier::Full), 50);
        assert_eq!(sidebar_width(WidthTier::Medium), 40);
        assert_eq!(sidebar_width(WidthTier::Minimum), 30);
        assert_eq!(sidebar_width(WidthTier::TooSmall), 0);
    }
}
