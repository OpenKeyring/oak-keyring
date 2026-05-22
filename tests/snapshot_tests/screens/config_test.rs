use ratatui::backend::TestBackend;
use ratatui::Terminal;

use chrono::TimeZone;
use oak_keyring::config::{GoogleDriveConfig, ProviderConfig, SyncMode, SyncProvider};
use oak_keyring::tui::screens::config_screen::ConfigScreen;
use oak_keyring::tui::state::config_state::{
    ConfigOverlay, ConfigTab, ConfirmButton, DropdownField, GDriveAuthStatus, SyncConnectionStatus,
};
use oak_keyring::tui::traits::screen::Screen;

use crate::support::snapshot_locale;

fn render_screen(screen: &ConfigScreen, width: u16, height: u16) -> TestBackend {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            screen.view(frame, frame.area());
        })
        .unwrap();
    terminal.backend().clone()
}

fn configure_google_drive_sync(screen: &mut ConfigScreen) {
    screen.state.active_tab = ConfigTab::Sync;
    screen.state.sync.provider = SyncProvider::GoogleDrive;
    screen.state.sync.provider_config = Some(ProviderConfig::GoogleDrive(GoogleDriveConfig {
        root_path: "/OpenKeyring".to_string(),
        ..GoogleDriveConfig::default()
    }));
}

#[test]
fn config_general_tab() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.active_tab = ConfigTab::General;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_general_tab", backend);
}

#[test]
fn config_sync_tab() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.active_tab = ConfigTab::Sync;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_sync_tab", backend);
}

#[test]
fn config_sync_disabled_state() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.active_tab = ConfigTab::Sync;
    screen.state.sync.provider = SyncProvider::Disabled;
    screen.state.sync.sync_mode = SyncMode::Auto;
    screen.state.sync_status = SyncConnectionStatus::NotConfigured;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_sync_disabled_state", backend);
}

#[test]
fn config_sync_disconnected_auto_state() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    configure_google_drive_sync(&mut screen);
    screen.state.sync.sync_mode = SyncMode::Auto;
    screen.state.sync.auto_interval_seconds = 300;
    screen.state.sync_status = SyncConnectionStatus::Disconnected;
    screen.state.sync_error_message = Some("OAuth token expired".to_string());
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_sync_disconnected_auto_state", backend);
}

#[test]
fn config_sync_testing_state() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    configure_google_drive_sync(&mut screen);
    screen.state.sync_status = SyncConnectionStatus::Testing;
    screen.state.gdrive_auth_status = GDriveAuthStatus::Authorizing;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_sync_testing_state", backend);
}

#[test]
fn config_sync_connected_state() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    configure_google_drive_sync(&mut screen);
    screen.state.sync_status = SyncConnectionStatus::Connected;
    screen.state.gdrive_auth_status = GDriveAuthStatus::Authorized;
    screen.state.last_sync = Some(
        chrono::Utc
            .with_ymd_and_hms(2026, 5, 20, 12, 30, 0)
            .unwrap(),
    );
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_sync_connected_state", backend);
}

#[test]
fn config_sync_manual_interval_disabled_state() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    configure_google_drive_sync(&mut screen);
    screen.state.focused_item = 2;
    screen.state.sync.sync_mode = SyncMode::Manual;
    screen.state.sync.auto_interval_seconds = 1800;
    screen.state.sync_status = SyncConnectionStatus::Disconnected;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_sync_manual_interval_disabled_state", backend);
}

#[test]
fn config_security_tab() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.active_tab = ConfigTab::Security;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_security_tab", backend);
}

#[test]
fn config_password_tab() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.active_tab = ConfigTab::Password;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_password_tab", backend);
}

#[test]
fn config_about_tab() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.active_tab = ConfigTab::About;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_about_tab", backend);
}

#[test]
fn config_dropdown_overlay() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.active_tab = ConfigTab::General;
    screen.state.overlay = Some(ConfigOverlay::Dropdown {
        field: DropdownField::Language,
        options: vec!["English".to_string(), "中文 (Chinese)".to_string()],
        selected: 0,
    });
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_dropdown_overlay", backend);
}

#[test]
fn config_unsaved_changes_dialog() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.overlay = Some(ConfigOverlay::UnsavedChanges {
        focused_button: ConfirmButton::Confirm,
    });
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_unsaved_changes_dialog", backend);
}
