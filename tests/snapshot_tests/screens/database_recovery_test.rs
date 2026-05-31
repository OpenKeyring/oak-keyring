use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::tui::screens::database_recovery::{
    DatabaseRecoveryFocus, DatabaseRecoveryMode, DatabaseRecoveryOrigin, DatabaseRecoveryScreen,
};
use oak_keyring::tui::traits::screen::Screen;
use oak_keyring::types::sensitive::SensitiveInput;

use crate::support::snapshot_locale;

fn sensitive(s: &str) -> SensitiveInput {
    let mut input = SensitiveInput::new();
    for c in s.chars() {
        input.push_char(c);
    }
    input
}

fn render_screen(screen: &DatabaseRecoveryScreen, width: u16, height: u16) -> TestBackend {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            screen.view(frame, frame.area());
        })
        .unwrap();
    terminal.backend().clone()
}

#[test]
fn db_recovery_source_selection_cloud_focused() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        origin: DatabaseRecoveryOrigin::StartupKeyOnly,
        mode: DatabaseRecoveryMode::SourceSelection,
        focus: DatabaseRecoveryFocus::Cloud,
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly)
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("db_recovery_source_selection_cloud_focused", backend);
}

#[test]
fn db_recovery_source_selection_shows_both_restore_sources() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        origin: DatabaseRecoveryOrigin::OnboardingRestore,
        mode: DatabaseRecoveryMode::SourceSelection,
        focus: DatabaseRecoveryFocus::Cloud,
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::OnboardingRestore)
    };

    let backend = render_screen(&screen, 80, 24);
    let rendered = format!("{backend:?}");

    assert!(
        rendered.contains("Restore from Cloud Sync"),
        "cloud restore source should be visible"
    );
    assert!(
        rendered.contains("Restore from .okb Backup"),
        ".okb restore source should be visible"
    );
}

#[test]
fn db_recovery_source_selection_okb_focused() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        origin: DatabaseRecoveryOrigin::OnboardingRestore,
        mode: DatabaseRecoveryMode::SourceSelection,
        focus: DatabaseRecoveryFocus::Okb,
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::OnboardingRestore)
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("db_recovery_source_selection_okb_focused", backend);
}

#[test]
fn db_recovery_okb_path_input() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        mode: DatabaseRecoveryMode::OkbPathInput,
        okb_path: "/path/to/my/backup.okb".to_string(),
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly)
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("db_recovery_okb_path_input", backend);
}

#[test]
fn db_recovery_okb_password_input() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        mode: DatabaseRecoveryMode::OkbPasswordInput,
        okb_password: sensitive("backuppassword"),
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly)
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("db_recovery_okb_password_input", backend);
}

#[test]
fn db_recovery_okb_master_password_input() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        mode: DatabaseRecoveryMode::OkbMasterPasswordInput,
        master_password: sensitive("masterpassword"),
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly)
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("db_recovery_okb_master_password_input", backend);
}

#[test]
fn db_recovery_cloud_master_password_input() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        mode: DatabaseRecoveryMode::CloudMasterPasswordInput,
        master_password: sensitive("masterpassword"),
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly)
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("db_recovery_cloud_master_password_input", backend);
}

#[test]
fn db_recovery_cloud_syncing() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        mode: DatabaseRecoveryMode::CloudSyncing,
        progress: Some((25, 100, "Downloading vault.db...".to_string())),
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly)
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("db_recovery_cloud_syncing", backend);
}

#[test]
fn db_recovery_cloud_needs_oauth() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        mode: DatabaseRecoveryMode::CloudNeedsOAuth,
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly)
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("db_recovery_cloud_needs_oauth", backend);
}

#[test]
fn db_recovery_cloud_failed() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        mode: DatabaseRecoveryMode::CloudFailed,
        error: Some("Network connection timeout".to_string()),
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly)
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("db_recovery_cloud_failed", backend);
}

#[test]
fn db_recovery_cloud_succeeded() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        mode: DatabaseRecoveryMode::CloudSucceeded,
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly)
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("db_recovery_cloud_succeeded", backend);
}

#[test]
fn db_recovery_okb_succeeded() {
    let _locale = snapshot_locale();
    let screen = DatabaseRecoveryScreen {
        mode: DatabaseRecoveryMode::OkbSucceeded,
        ..DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly)
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("db_recovery_okb_succeeded", backend);
}
