pub mod animation;
pub mod audit_state;
pub mod config_state;
pub mod detail_state;
pub mod focus;
pub mod form_state;
pub mod generator_state;
pub mod list_state;
pub mod loading;
pub mod main_state;
pub mod notification;
pub mod overlay_state;
pub mod screen_restore;
pub mod sync_ui_state;
pub mod tag_management;

use crate::commands::types::{AppPhase, Screen};
use crate::tui::screens::audit_log::AuditLogScreen;
use crate::tui::screens::change_master_password::ChangeMasterPasswordScreen;
use crate::tui::screens::config_screen::ConfigScreen;
use crate::tui::screens::create_record::CreateRecordScreen;
use crate::tui::screens::edit_record::EditRecordScreen;
use crate::tui::screens::import_export::ImportExportScreen;
use crate::tui::screens::onboarding::OnboardingScreen;
use crate::tui::screens::password_generator::PasswordGeneratorScreen;
use crate::tui::screens::set_password::SetPasswordScreen;
use crate::tui::screens::sync_conflict::SyncConflictScreen;
use crate::tui::screens::unlock::UnlockScreen;
use main_state::MainScreenState;

pub use screen_restore::{
    AuditLogRestoreState, ConfigRestoreState, ImportExportRestoreState, MainRestoreState,
    ScreenRestoreState, ScreenSnapshot,
};

/// Central application state. Owned by `App`, passed by `&mut` to update() and `&` to view().
pub struct AppState {
    pub phase: AppPhase,
    pub shared: SharedState,
    pub screens: ScreenStates,
    /// Current active screen
    pub current_screen: Screen,
    /// Screen snapshot history for focus/scroll restoration on go_back
    pub screen_history: Vec<ScreenSnapshot>,
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
#[derive(Default)]
pub struct SharedState {
    pub notification: notification::NotificationState,
    pub loading: loading::LoadingState,
    pub focus: focus::FocusState,
    pub animation: animation::AnimationState,
    /// Timestamp of the last successful sync, shared across screens.
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
}

/// Per-screen state containers. Only one is active at a time (determined by current_screen).
#[derive(Default)]
pub struct ScreenStates {
    pub unlock: UnlockScreen,
    pub onboarding: OnboardingScreen,
    pub main: MainScreenState,
    pub config: ConfigScreen,
    pub change_master_password: ChangeMasterPasswordScreen,
    pub set_new_master_password: SetPasswordScreen,
    pub import_export: ImportExportScreen,
    pub audit_log: AuditLogScreen,
    pub sync_conflict: SyncConflictScreen,
    pub password_generator: PasswordGeneratorScreen,
    pub create_record: CreateRecordScreen,
    pub edit_record: EditRecordScreen,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            phase: AppPhase::Initializing,
            shared: SharedState::default(),
            screens: ScreenStates::default(),
            current_screen: Screen::Unlock,
            screen_history: Vec::new(),
            terminal_size: (80, 24),
            too_small: false,
            unicode_capable: true,
            signal_count: 0,
            last_signal_time: None,
        }
    }
}

impl AppState {
    /// Create a new AppState with the initial screen determined by vault file state.
    ///
    /// Four states per spec:
    /// - key + db → UnlockScreen
    /// - no key, no db → OnboardingScreen
    /// - no key, db → OnboardingScreen (Restore path — recovery word restore)
    /// - key, no db → OnboardingScreen (CreateNew path — DB re-initialization)
    pub fn new(has_vault: bool, vault_has_key_only: bool, vault_has_db_only: bool) -> Self {
        let initial_screen = if has_vault && !vault_has_key_only && !vault_has_db_only {
            Screen::Unlock
        } else {
            Screen::Onboarding
        };

        Self {
            phase: AppPhase::Initializing,
            shared: SharedState::default(),
            screens: ScreenStates::default(),
            current_screen: initial_screen,
            screen_history: Vec::new(),
            terminal_size: (80, 24),
            too_small: false,
            unicode_capable: true,
            signal_count: 0,
            last_signal_time: None,
        }
    }

    /// Check if terminal meets minimum size requirement (80x24).
    pub fn update_size(&mut self, width: u16, height: u16) {
        self.terminal_size = (width, height);
        self.too_small = width < 80 || height < 24;
    }
}
