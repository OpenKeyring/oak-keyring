pub mod database_recovery;
pub mod key_recovery;
pub mod main;
pub mod onboarding;
pub mod recovery_key;
pub mod unlock;

pub mod audit_log;
pub mod change_master_password;
pub mod config_screen;
pub mod create_record;
pub mod edit_record;
pub mod form;
pub mod import_export;
pub mod password_generator;
pub mod set_password;
pub mod sync_conflict;

// Re-export Screen trait for convenience.
pub use crate::tui::traits::screen::Screen;
