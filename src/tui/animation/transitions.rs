//! Page transition orchestration -- timing and effect selection per scenario.
//!
//! Centralises the mapping from `EffectKind` to concrete timing parameters
//! so that screen switching code only needs to call `start_transition`.

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
/// Each `EffectKind` variant maps to a predefined duration and whether
/// the transition is interruptible by a newer transition.
pub fn start_transition(state: &mut AnimationState, kind: EffectKind) {
    let (duration, interruptible) = match kind {
        EffectKind::UnlockTransition => (timing::UNLOCK_TO_MAIN, false),
        EffectKind::LockTransition => (timing::MAIN_TO_LOCK, false),
        EffectKind::PageSwitch => (timing::PAGE_SWITCH, true),
        EffectKind::ModalAppear => (timing::MODAL_APPEAR, true),
        EffectKind::ModalDismiss => (timing::MODAL_DISMISS, true),
        EffectKind::BrandDissolve => (timing::UNLOCK_TO_MAIN, false),
        EffectKind::ScreenIn => (timing::SCREEN_IN, true),
        EffectKind::ScreenOut => (timing::SCREEN_OUT, true),
    };
    state.start(kind, duration, interruptible);
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
            EffectKind::ScreenIn,
            EffectKind::ScreenOut,
        ] {
            state.clear();
            start_transition(&mut state, kind);
            assert!(state.is_active(), "EffectKind::{kind:?} should be active");
        }
    }
}
