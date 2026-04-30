use crate::tui::animation::AnimationLevel;

#[derive(Debug)]
pub struct AnimationState {
    pub level: AnimationLevel,
    /// Currently active transition effect (if any)
    pub active_effect: Option<ActiveEffect>,
}

#[derive(Debug)]
pub struct ActiveEffect {
    pub kind: EffectKind,
    pub effect: tachyonfx::Effect,
    pub started_at: std::time::Instant,
    pub duration_ms: u64,
    pub interruptible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    UnlockTransition,
    LockTransition,
    PageSwitch,
    ModalAppear,
    ModalDismiss,
    BrandDissolve,
    ScreenIn,
    ScreenOut,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            level: AnimationLevel::Full, // Will be overridden by detect
            active_effect: None,
        }
    }
}

impl AnimationState {
    pub fn start(
        &mut self,
        kind: EffectKind,
        effect: tachyonfx::Effect,
        duration_ms: u64,
        interruptible: bool,
    ) {
        if self.level == AnimationLevel::None {
            return;
        }
        if self
            .active_effect
            .as_ref()
            .is_some_and(|active| {
                !active.interruptible
                    && active.started_at.elapsed().as_millis()
                        < active.duration_ms as u128
            })
        {
            return;
        }
        self.active_effect = Some(ActiveEffect {
            kind,
            effect,
            started_at: std::time::Instant::now(),
            duration_ms,
            interruptible,
        });
    }

    pub fn is_active(&self) -> bool {
        self.active_effect
            .as_ref()
            .is_some_and(|e| e.started_at.elapsed().as_millis() < e.duration_ms as u128)
    }

    pub fn clear(&mut self) {
        self.active_effect = None;
    }

    pub fn clear_finished(&mut self) {
        if !self.is_active() {
            self.active_effect = None;
        }
    }
}

#[cfg(test)]
impl AnimationState {
    pub fn has_active_kind(&self, kind: EffectKind) -> bool {
        self.active_effect
            .as_ref()
            .is_some_and(|active| active.kind == kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_finished_removes_expired_effect() {
        let mut state = AnimationState::default();
        crate::tui::animation::transitions::start_transition(
            &mut state,
            EffectKind::ScreenIn,
        );
        if let Some(active) = state.active_effect.as_mut() {
            active.started_at = std::time::Instant::now()
                - std::time::Duration::from_millis(active.duration_ms + 1);
        }

        state.clear_finished();

        assert!(state.active_effect.is_none());
    }
}
