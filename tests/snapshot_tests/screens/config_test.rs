use ratatui::backend::TestBackend;
use ratatui::Terminal;

use chrono::TimeZone;
use oak_keyring::config::{
    GoogleDriveConfig, PasswordGenerationStyle, ProviderConfig, SyncMode, SyncProvider,
};
use oak_keyring::tui::screens::config_screen::ConfigScreen;
use oak_keyring::tui::state::config_state::{
    ConfigOverlay, ConfigTab, ConfirmButton, DropdownField, GDriveAuthStatus, SyncConnectionStatus,
};
use oak_keyring::tui::theme;
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

fn backend_text(backend: &TestBackend) -> String {
    let buffer = backend.buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).expect("cell").symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_position(backend: &TestBackend, text: &str) -> Option<(u16, u16)> {
    let buffer = backend.buffer();
    let target: Vec<String> = text.chars().map(|ch| ch.to_string()).collect();
    if target.is_empty() {
        return None;
    }
    for y in 0..buffer.area.height {
        let symbols: Vec<&str> = (0..buffer.area.width)
            .map(|x| buffer.cell((x, y)).expect("cell").symbol())
            .collect();
        if let Some(start) = symbols
            .windows(target.len())
            .position(|window| window.iter().zip(&target).all(|(cell, ch)| *cell == ch))
        {
            return Some((start as u16, y));
        }
    }
    None
}

fn cell_at_text_start<'a>(
    backend: &'a TestBackend,
    text: &str,
) -> Option<&'a ratatui::buffer::Cell> {
    let buffer = backend.buffer();
    let target: Vec<String> = text.chars().map(|ch| ch.to_string()).collect();
    if target.is_empty() {
        return None;
    }
    for y in 0..buffer.area.height {
        let symbols: Vec<&str> = (0..buffer.area.width)
            .map(|x| buffer.cell((x, y)).expect("cell").symbol())
            .collect();
        if let Some(start) = symbols
            .windows(target.len())
            .position(|window| window.iter().zip(&target).all(|(cell, ch)| *cell == ch))
        {
            return buffer.cell((start as u16, y));
        }
    }
    None
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
fn config_general_newlook_chrome_and_controls() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.active_tab = ConfigTab::General;
    let backend = render_screen(&screen, 120, 30);
    let rendered = backend_text(&backend);

    assert!(
        rendered.contains(theme::NF_GEAR),
        "config tab bar should use the new-look gear icon"
    );
    assert!(
        rendered.contains(theme::NF_GLOBE),
        "general rows should use field icons"
    );
    assert!(
        rendered.contains("\u{f093}"),
        "import action should use the upload icon"
    );
    assert!(
        rendered.contains("\u{f019}"),
        "export action should use the download icon"
    );
    assert!(
        rendered.contains("┌") && rendered.contains("┘"),
        "config screen should render bordered panels"
    );
    assert!(
        rendered.contains("\u{258C}  General"),
        "content panel should render a marked General section title"
    );
    assert!(
        rendered.contains("[  auto") || rendered.contains("[ auto"),
        "dropdown values should render as bracket controls"
    );
}

#[test]
fn config_footer_close_focus_uses_newlook_danger_button() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.footer_focus = Some(oak_keyring::tui::state::config_state::FooterButton::Close);
    let backend = render_screen(&screen, 120, 30);
    let close_cell =
        cell_at_text_start(&backend, "Close").expect("focused close button should render");

    assert_eq!(close_cell.style().fg, Some(theme::NL_DANGER));
    assert_eq!(close_cell.style().bg, Some(theme::NL_SURFACE_2));
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
fn config_password_tab_shows_all_generator_defaults() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.active_tab = ConfigTab::Password;
    screen.state.password.style = PasswordGenerationStyle::Memorable;
    let backend = render_screen(&screen, 120, 30);
    let rendered = backend_text(&backend);

    assert!(rendered.contains("Default Style"));
    assert!(rendered.contains("Random Length"));
    assert!(rendered.contains("Lowercase"));
    assert!(rendered.contains("Memorable Words"));
    assert!(rendered.contains("Capitalize Words"));
    assert!(rendered.contains("Separator"));
    assert!(rendered.contains("PIN Length"));
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
fn config_about_content_aligns_with_title_and_has_product_context() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    screen.state.active_tab = ConfigTab::About;
    let backend = render_screen(&screen, 120, 30);
    let rendered = backend_text(&backend);

    let about_marker = text_position(&backend, "▌  About").expect("About title should render");
    let version_label = text_position(&backend, "Version").expect("Version label should render");

    assert_eq!(
        version_label.0,
        about_marker.0 + 3,
        "about metadata rows should align with the section title text"
    );
    assert!(rendered.contains("OpenKeyring / oak-keyring"));
    assert!(rendered.contains("Local-first"));
    assert!(rendered.contains("Encrypted vault"));
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
        focused_button: ConfirmButton::SaveExit,
    });
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("config_unsaved_changes_dialog", backend);
}

#[test]
fn config_footer_names_reverse_tab_navigation() {
    let _locale = snapshot_locale();
    let screen = ConfigScreen::new();
    let backend = render_screen(&screen, 120, 30);
    let rendered = backend_text(&backend);

    assert!(rendered.contains("Shift+Tab"));
}

#[test]
fn config_sync_google_drive_authorize_row_is_focusable() {
    let _locale = snapshot_locale();
    let mut screen = ConfigScreen::new();
    configure_google_drive_sync(&mut screen);
    screen.state.focused_item = 3;
    screen.state.gdrive_auth_status = GDriveAuthStatus::NotAuthorized;
    let backend = render_screen(&screen, 120, 30);

    let auth_cell = cell_at_text_start(&backend, "Authorize")
        .expect("Google Drive authorize action should render");
    assert_eq!(auth_cell.style().bg, Some(theme::NL_SELECTED));

    let rendered = backend_text(&backend);
    assert!(rendered.contains("Google Drive Folder"));
}
