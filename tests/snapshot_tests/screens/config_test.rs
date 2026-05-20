use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::tui::screens::config_screen::ConfigScreen;
use oak_keyring::tui::state::config_state::{
    ConfigOverlay, ConfigTab, ConfirmButton, DropdownField,
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
