pub mod animation;
pub mod focus;
pub mod loading;
pub mod notification;

use crate::commands::types::{AppPhase, Screen};
use crate::tui::screens::onboarding::OnboardingScreen;
use crate::tui::screens::unlock::UnlockScreen;

/// Central application state. Owned by `App`, passed by `&mut` to update() and `&` to view().
pub struct AppState {
    pub phase: AppPhase,
    pub shared: SharedState,
    pub screens: ScreenStates,
    /// Current active screen
    pub current_screen: Screen,
    /// Screen navigation stack for GoBack
    pub screen_stack: Vec<Screen>,
    /// Terminal dimensions (updated on Resize event)
    pub terminal_size: (u16, u16),
    /// Whether terminal is too small for main UI
    pub too_small: bool,
    /// Unicode capability detected at startup
    pub unicode_capable: bool,
    /// Signal handler state
    pub signal_count: u8,
    pub last_signal_time: Option<std::time::Instant>,
}

/// Cross-cutting state shared across all screens.
pub struct SharedState {
    pub notification: notification::NotificationState,
    pub loading: loading::LoadingState,
    pub focus: focus::FocusState,
    pub animation: animation::AnimationState,
}

/// Per-screen state containers. Only one is active at a time (determined by current_screen).
pub struct ScreenStates {
    pub unlock: UnlockScreen,
    pub onboarding: OnboardingScreen,
    // Main screen state will be added in Plan M
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            phase: AppPhase::Initializing,
            shared: SharedState::default(),
            screens: ScreenStates::default(),
            current_screen: Screen::Unlock,
            screen_stack: Vec::new(),
            terminal_size: (80, 24),
            too_small: false,
            unicode_capable: true,
            signal_count: 0,
            last_signal_time: None,
        }
    }
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            notification: notification::NotificationState::default(),
            loading: loading::LoadingState::default(),
            focus: focus::FocusState::default(),
            animation: animation::AnimationState::default(),
        }
    }
}

impl Default for ScreenStates {
    fn default() -> Self {
        Self {
            unlock: UnlockScreen::default(),
            onboarding: OnboardingScreen::default(),
        }
    }
}

impl AppState {
    /// Navigate to a new screen, pushing current onto the stack.
    pub fn navigate_to(&mut self, screen: Screen) {
        self.screen_stack.push(self.current_screen);
        self.current_screen = screen;
    }

    /// Go back to previous screen. Returns false if stack is empty.
    pub fn go_back(&mut self) -> bool {
        if let Some(prev) = self.screen_stack.pop() {
            self.current_screen = prev;
            true
        } else {
            false
        }
    }

    /// Check if terminal meets minimum size requirement (80x24).
    pub fn update_size(&mut self, width: u16, height: u16) {
        self.terminal_size = (width, height);
        self.too_small = width < 80 || height < 24;
    }
}
