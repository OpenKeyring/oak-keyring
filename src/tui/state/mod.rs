pub mod animation;
pub mod focus;
pub mod loading;
pub mod notification;

use crate::commands::AppPhase;

pub struct AppState {
    pub phase: AppPhase,
}

pub struct ScreenStates {
    // TODO: Add per-screen state containers
}

pub struct SharedState {
    // TODO: Add cross-cutting state (notification, animation, loading)
}
