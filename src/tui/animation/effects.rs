//! tachyonfx effect builders for transition animations.
//!
//! Wraps tachyonfx effects with animation-level awareness.

use crate::tui::animation::AnimationLevel;

/// Create a dissolve effect for brand transitions (unlock to main).
/// Returns None if animation level is None.
pub fn dissolve(duration_ms: u32, level: AnimationLevel) -> Option<tachyonfx::Effect> {
    if level == AnimationLevel::None {
        return None;
    }
    Some(tachyonfx::fx::dissolve(duration_ms))
}

/// Create a coalesce effect for reverse brand transitions.
pub fn coalesce(duration_ms: u32, level: AnimationLevel) -> Option<tachyonfx::Effect> {
    if level == AnimationLevel::None {
        return None;
    }
    Some(tachyonfx::fx::coalesce(duration_ms))
}

/// Create a slide-in effect for page switches.
pub fn slide_in(duration_ms: u32, level: AnimationLevel) -> Option<tachyonfx::Effect> {
    if level == AnimationLevel::None {
        return None;
    }
    Some(tachyonfx::fx::slide_in(
        tachyonfx::Motion::LeftToRight,
        20,
        0,
        ratatui::style::Color::Black,
        (duration_ms, tachyonfx::Interpolation::Linear),
    ))
}

/// Create an expand-vertical effect for modal popups.
pub fn expand_vertical(duration_ms: u32, level: AnimationLevel) -> Option<tachyonfx::Effect> {
    if level == AnimationLevel::None {
        return None;
    }
    Some(tachyonfx::fx::expand(
        tachyonfx::fx::ExpandDirection::Vertical,
        ratatui::style::Style::default().bg(ratatui::style::Color::Black),
        (duration_ms, tachyonfx::Interpolation::CubicInOut),
    ))
}
