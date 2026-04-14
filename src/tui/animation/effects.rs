//! tachyonfx effect builders for transition animations.
//!
//! Wraps tachyonfx effects with animation-level awareness and simplified APIs
//! tailored to the app's transition scenarios.
//!
//! NOTE: tachyonfx 0.20.1 depends on ratatui 0.29.x while the project uses
//! ratatui 0.30.x. To avoid type mismatches, we only call tachyonfx functions
//! that accept primitive parameters (timers as `u32` ms values). Functions
//! requiring ratatui `Color`/`Style` are stubbed with TODO comments until the
//! tachyonfx version is upgraded to match the project's ratatui version.

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
///
/// Currently returns a dissolve effect as a placeholder because the tachyonfx
/// `slide_in` function requires ratatui `Color` which is incompatible across
/// the ratatui 0.29/0.30 version boundary. Will be updated once tachyonfx
/// aligns with ratatui 0.30.
// TODO: Replace with actual slide_in once tachyonfx supports ratatui 0.30
pub fn slide_in(duration_ms: u32, level: AnimationLevel) -> Option<tachyonfx::Effect> {
    if level == AnimationLevel::None {
        return None;
    }
    Some(tachyonfx::fx::dissolve(duration_ms))
}

/// Create an expand-vertical effect for modal popups.
///
/// Currently uses dissolve as a placeholder because the tachyonfx `expand`
/// and `fade_from_fg` functions require ratatui `Style`/`Color` types that
/// are incompatible across the ratatui 0.29/0.30 version boundary.
// TODO: Replace with actual expand/fade once tachyonfx supports ratatui 0.30
pub fn expand_vertical(duration_ms: u32, level: AnimationLevel) -> Option<tachyonfx::Effect> {
    if level == AnimationLevel::None {
        return None;
    }
    Some(tachyonfx::fx::dissolve(duration_ms))
}
