use crate::tui::animation::AnimationLevel;

#[derive(Debug, Clone)]
pub struct AnimationState {
    pub level: AnimationLevel,
    /// Currently active transition effect (if any)
    pub active_effect: Option<ActiveEffect>,
}

#[derive(Debug, Clone)]
pub struct ActiveEffect {
    pub kind: EffectKind,
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
    pub fn start(&mut self, kind: EffectKind, duration_ms: u64, interruptible: bool) {
        if self.level == AnimationLevel::None {
            return;
        }
        self.active_effect = Some(ActiveEffect {
            kind,
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
}
