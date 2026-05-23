//! Page transition orchestration -- timing and effect selection per scenario.
//!
//! Centralises the mapping from `EffectKind` to concrete timing parameters and
//! tachyonfx effects so that screen switching code only needs to call `start_transition`.

use crate::tui::state::animation::{AnimationState, EffectKind};

/// Transition timing constants (ms).
pub mod timing {
    /// Unlock screen -> main screen brand transition.
    pub const UNLOCK_TO_MAIN: u64 = 1000;
    /// Main screen -> lock screen transition.
    pub const MAIN_TO_LOCK: u64 = 900;
    /// Switching between pages in the main layout.
    pub const PAGE_SWITCH: u64 = 500;
    /// Modal overlay appearing.
    pub const MODAL_APPEAR: u64 = 200;
    /// Modal overlay dismissing.
    pub const MODAL_DISMISS: u64 = 150;
    /// First-run onboarding intro.
    pub const ONBOARDING_INTRO: u64 = 2000;
    /// Onboarding step transition.
    pub const ONBOARDING_STEP: u64 = 400;
    /// Sidebar sweep animation.
    pub const SIDEBAR_SWEEP: u64 = 200;
    /// Screen transition in.
    pub const SCREEN_IN: u64 = 300;
    /// Screen transition out.
    pub const SCREEN_OUT: u64 = 200;
}

/// Start a transition effect on the given `AnimationState`.
///
/// Each `EffectKind` variant maps to a predefined duration, interruptibility,
/// and concrete tachyonfx effect.
pub fn start_transition(state: &mut AnimationState, kind: EffectKind) {
    use crate::tui::animation::AnimationLevel;

    if state.level == AnimationLevel::None {
        return;
    }

    let (duration, interruptible, effect) = match kind {
        EffectKind::UnlockTransition => (
            timing::UNLOCK_TO_MAIN,
            false,
            crate::tui::animation::effects::dissolve(timing::UNLOCK_TO_MAIN as u32, state.level),
        ),
        EffectKind::LockTransition => (
            timing::MAIN_TO_LOCK,
            false,
            crate::tui::animation::effects::coalesce(timing::MAIN_TO_LOCK as u32, state.level),
        ),
        EffectKind::PageSwitch => (
            timing::PAGE_SWITCH,
            true,
            crate::tui::animation::effects::slide_in(timing::PAGE_SWITCH as u32, state.level),
        ),
        EffectKind::ModalAppear => (
            timing::MODAL_APPEAR,
            true,
            crate::tui::animation::effects::expand_vertical(
                timing::MODAL_APPEAR as u32,
                state.level,
            ),
        ),
        EffectKind::ModalDismiss => (
            timing::MODAL_DISMISS,
            true,
            crate::tui::animation::effects::dissolve(timing::MODAL_DISMISS as u32, state.level),
        ),
        EffectKind::BrandDissolve => (
            timing::UNLOCK_TO_MAIN,
            false,
            crate::tui::animation::effects::dissolve(timing::UNLOCK_TO_MAIN as u32, state.level),
        ),
        EffectKind::OnboardingIntro => (
            timing::ONBOARDING_INTRO,
            true,
            crate::tui::animation::effects::onboarding_intro(
                timing::ONBOARDING_INTRO as u32,
                state.level,
            ),
        ),
        EffectKind::OnboardingForward => (
            timing::ONBOARDING_STEP,
            true,
            crate::tui::animation::effects::onboarding_forward(
                timing::ONBOARDING_STEP as u32,
                state.level,
            ),
        ),
        EffectKind::OnboardingBack => (
            timing::ONBOARDING_STEP,
            true,
            crate::tui::animation::effects::onboarding_back(
                timing::ONBOARDING_STEP as u32,
                state.level,
            ),
        ),
        EffectKind::ScreenIn => (
            timing::SCREEN_IN,
            true,
            crate::tui::animation::effects::coalesce(timing::SCREEN_IN as u32, state.level),
        ),
        EffectKind::ScreenOut => (
            timing::SCREEN_OUT,
            true,
            crate::tui::animation::effects::dissolve(timing::SCREEN_OUT as u32, state.level),
        ),
    };

    if let Some(effect) = effect {
        state.start(kind, effect, duration, interruptible);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::animation::AnimationLevel;

    #[test]
    fn start_transition_sets_active_effect() {
        let mut state = AnimationState::default();
        assert!(!state.is_active());

        start_transition(&mut state, EffectKind::PageSwitch);
        assert!(state.is_active());

        let effect = state.active_effect.as_ref().unwrap();
        assert_eq!(effect.kind, EffectKind::PageSwitch);
        assert_eq!(effect.duration_ms, timing::PAGE_SWITCH);
        assert!(effect.interruptible);
    }

    #[test]
    fn start_transition_respects_animation_none() {
        let mut state = AnimationState {
            level: AnimationLevel::None,
            active_effect: None,
        };
        start_transition(&mut state, EffectKind::UnlockTransition);
        assert!(!state.is_active());
    }

    #[test]
    fn all_effect_kinds_have_timing() {
        let mut state = AnimationState::default();

        for kind in [
            EffectKind::UnlockTransition,
            EffectKind::LockTransition,
            EffectKind::PageSwitch,
            EffectKind::ModalAppear,
            EffectKind::ModalDismiss,
            EffectKind::BrandDissolve,
            EffectKind::OnboardingIntro,
            EffectKind::OnboardingForward,
            EffectKind::OnboardingBack,
            EffectKind::ScreenIn,
            EffectKind::ScreenOut,
        ] {
            state.clear();
            start_transition(&mut state, kind);
            assert!(state.is_active(), "EffectKind::{kind:?} should be active");
        }
    }

    #[test]
    fn onboarding_transition_kinds_have_expected_timing() {
        let mut state = AnimationState::default();

        start_transition(&mut state, EffectKind::OnboardingIntro);
        let effect = state.active_effect.as_ref().expect("intro effect");
        assert_eq!(effect.kind, EffectKind::OnboardingIntro);
        assert_eq!(effect.duration_ms, 2000);
        assert_eq!(effect.duration_ms, timing::ONBOARDING_INTRO);
        assert!(effect.interruptible);

        state.clear();
        start_transition(&mut state, EffectKind::OnboardingForward);
        let effect = state.active_effect.as_ref().expect("forward effect");
        assert_eq!(effect.kind, EffectKind::OnboardingForward);
        assert_eq!(effect.duration_ms, timing::ONBOARDING_STEP);
        assert!(effect.interruptible);

        state.clear();
        start_transition(&mut state, EffectKind::OnboardingBack);
        let effect = state.active_effect.as_ref().expect("back effect");
        assert_eq!(effect.kind, EffectKind::OnboardingBack);
        assert_eq!(effect.duration_ms, timing::ONBOARDING_STEP);
        assert!(effect.interruptible);
    }

    #[test]
    fn onboarding_intro_can_be_interrupted_by_step_motion() {
        let mut state = AnimationState::default();

        start_transition(&mut state, EffectKind::OnboardingIntro);
        start_transition(&mut state, EffectKind::OnboardingForward);

        assert!(state.has_active_kind(EffectKind::OnboardingForward));
    }

    #[test]
    fn onboarding_intro_respects_reduced_and_none_levels() {
        let mut reduced = AnimationState {
            level: AnimationLevel::Reduced,
            active_effect: None,
        };
        start_transition(&mut reduced, EffectKind::OnboardingIntro);
        assert!(reduced.has_active_kind(EffectKind::OnboardingIntro));

        let mut none = AnimationState {
            level: AnimationLevel::None,
            active_effect: None,
        };
        start_transition(&mut none, EffectKind::OnboardingIntro);
        assert!(!none.is_active());
    }

    #[test]
    fn screen_in_transition_stores_concrete_effect() {
        let mut state = AnimationState::default();

        start_transition(&mut state, EffectKind::ScreenIn);

        let effect = state.active_effect.as_ref().expect("effect should exist");
        assert_eq!(effect.kind, EffectKind::ScreenIn);
        assert_eq!(effect.duration_ms, timing::SCREEN_IN);
    }

    #[test]
    fn non_interruptible_effect_is_not_replaced_by_interruptible_effect() {
        let mut state = AnimationState::default();

        start_transition(&mut state, EffectKind::UnlockTransition);
        start_transition(&mut state, EffectKind::ScreenIn);

        let effect = state.active_effect.as_ref().expect("effect should exist");
        assert_eq!(effect.kind, EffectKind::UnlockTransition);
    }
}
