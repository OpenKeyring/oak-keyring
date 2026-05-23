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

/// Cyber first-run onboarding intro.
pub fn onboarding_intro(duration_ms: u32, level: AnimationLevel) -> Option<tachyonfx::Effect> {
    use ratatui::style::{Color, Style};
    use tachyonfx::pattern::SweepPattern;
    use tachyonfx::{fx, Interpolation};

    match level {
        AnimationLevel::None => None,
        AnimationLevel::Reduced => Some(
            fx::coalesce_from(
                Style::default().bg(Color::Black),
                (duration_ms, Interpolation::CubicInOut),
            )
            .with_pattern(SweepPattern::left_to_right(18)),
        ),
        AnimationLevel::Full => {
            let coalesce = fx::coalesce_from(
                Style::default().bg(Color::Black),
                (duration_ms, Interpolation::CubicInOut),
            )
            .with_pattern(SweepPattern::left_to_right(24));
            let hsl = fx::hsl_shift_fg([180.0, 35.0, 8.0], (duration_ms, Interpolation::Linear))
                .with_pattern(SweepPattern::left_to_right(24));
            Some(fx::parallel(&[coalesce, hsl]))
        }
    }
}

/// Secure forward motion for onboarding step progression.
pub fn onboarding_forward(duration_ms: u32, level: AnimationLevel) -> Option<tachyonfx::Effect> {
    if level == AnimationLevel::None {
        return None;
    }
    Some(tachyonfx::fx::slide_in(
        tachyonfx::Motion::RightToLeft,
        16,
        0,
        ratatui::style::Color::Black,
        (duration_ms, tachyonfx::Interpolation::CubicInOut),
    ))
}

/// Secure reverse motion for onboarding back navigation.
///
/// The app does not keep an old-screen buffer, so this uses a reverse slide-in
/// for the restored/current content instead of sliding stale content out.
pub fn onboarding_back(duration_ms: u32, level: AnimationLevel) -> Option<tachyonfx::Effect> {
    if level == AnimationLevel::None {
        return None;
    }
    Some(tachyonfx::fx::slide_in(
        tachyonfx::Motion::LeftToRight,
        16,
        0,
        ratatui::style::Color::Black,
        (duration_ms, tachyonfx::Interpolation::CubicInOut),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn onboarding_forward_effect_changes_fixed_buffer_at_fixed_tick() {
        let mut effect =
            onboarding_forward(400, AnimationLevel::Full).expect("forward effect should exist");
        let area = Rect::new(0, 0, 24, 3);
        let mut buffer = Buffer::with_lines(vec!["OPENKEYRING", "SETUP", "READY"]);
        let before = format!("{buffer:?}");

        effect.process(
            std::time::Duration::from_millis(200).into(),
            &mut buffer,
            area,
        );

        let after = format!("{buffer:?}");
        assert_ne!(before, after);
    }

    #[test]
    fn onboarding_effects_are_absent_when_animation_is_none() {
        assert!(onboarding_intro(900, AnimationLevel::None).is_none());
        assert!(onboarding_forward(400, AnimationLevel::None).is_none());
        assert!(onboarding_back(400, AnimationLevel::None).is_none());
    }
}
