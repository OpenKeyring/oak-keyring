use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::crypto::strength::evaluate_strength;
use oak_keyring::tui::screens::change_master_password::{
    ChangeMasterPasswordScreen, PasswordField,
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

fn render_screen(screen: &ChangeMasterPasswordScreen, width: u16, height: u16) -> TestBackend {
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
fn change_master_password_step1_empty() {
    let _locale = snapshot_locale();
    let screen = ChangeMasterPasswordScreen::new();
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("change_master_password_step1_empty", backend);
}

#[test]
fn change_master_password_step1_filled() {
    let _locale = snapshot_locale();
    let mut screen = ChangeMasterPasswordScreen::new();
    screen.current_password = sensitive("mycurrentpassword");
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("change_master_password_step1_filled", backend);
}

#[test]
fn change_master_password_step1_error() {
    let _locale = snapshot_locale();
    let mut screen = ChangeMasterPasswordScreen::new();
    screen.current_password = sensitive("wrongpass");
    screen.error_message = Some("Invalid master password".to_string());
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("change_master_password_step1_error", backend);
}

#[test]
fn change_master_password_step2_new_focused_empty() {
    let _locale = snapshot_locale();
    let mut screen = ChangeMasterPasswordScreen::new();
    screen.step = 2;
    screen.focused = PasswordField::New;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("change_master_password_step2_new_focused_empty", backend);
}

#[test]
fn change_master_password_step2_new_focused_strong() {
    let _locale = snapshot_locale();
    let mut screen = ChangeMasterPasswordScreen::new();
    screen.step = 2;
    screen.focused = PasswordField::New;
    let pw = "p@$$w0rdStr0ng123";
    screen.new_password = sensitive(pw);
    screen.password_strength = Some(evaluate_strength(pw));
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("change_master_password_step2_new_focused_strong", backend);
}

#[test]
fn change_master_password_step2_confirm_focused() {
    let _locale = snapshot_locale();
    let mut screen = ChangeMasterPasswordScreen::new();
    screen.step = 2;
    screen.focused = PasswordField::Confirm;
    let pw = "p@$$w0rdStr0ng123";
    screen.new_password = sensitive(pw);
    screen.password_strength = Some(evaluate_strength(pw));
    screen.confirm_password = sensitive("p@$$w0rd");
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("change_master_password_step2_confirm_focused", backend);
}

#[test]
fn change_master_password_step2_mismatch_error() {
    let _locale = snapshot_locale();
    let mut screen = ChangeMasterPasswordScreen::new();
    screen.step = 2;
    screen.focused = PasswordField::Confirm;
    let pw = "p@$$w0rdStr0ng123";
    screen.new_password = sensitive(pw);
    screen.password_strength = Some(evaluate_strength(pw));
    screen.confirm_password = sensitive("p@$$w0rd");
    screen.error_message = Some("Passwords do not match".to_string());
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("change_master_password_step2_mismatch_error", backend);
}
