use std::time::{Duration, Instant};

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::tui::screens::unlock::{UnlockMode, UnlockPhase, UnlockScreen};
use oak_keyring::tui::traits::screen::Screen;
use oak_keyring::types::sensitive::SensitiveInput;

fn sensitive(s: &str) -> SensitiveInput {
    let mut input = SensitiveInput::new();
    for c in s.chars() {
        input.push_char(c);
    }
    input
}

fn render_screen(screen: &UnlockScreen, width: u16, height: u16) -> TestBackend {
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
fn unlock_screen_empty_password_mode() {
    let mut screen = UnlockScreen::default();
    screen.mode = UnlockMode::Password;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("unlock_screen_empty_password_mode", backend);
}

#[test]
fn unlock_screen_with_password() {
    let mut screen = UnlockScreen::default();
    screen.mode = UnlockMode::Password;
    screen.password_input = sensitive("secretpassword");
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("unlock_screen_with_password", backend);
}

#[test]
fn unlock_screen_recovery_key_mode() {
    let mut screen = UnlockScreen::default();
    screen.mode = UnlockMode::RecoveryKey;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("unlock_screen_recovery_key_mode", backend);
}

#[test]
fn unlock_screen_failed_state() {
    let mut screen = UnlockScreen::default();
    screen.mode = UnlockMode::Password;
    screen.password_input = sensitive("wrong");
    screen.state = UnlockPhase::Failed;
    screen.error_message = Some("Wrong password. Please try again.".to_string());
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("unlock_screen_failed_state", backend);
}

#[test]
fn unlock_screen_verifying_state() {
    let mut screen = UnlockScreen::default();
    screen.mode = UnlockMode::Password;
    screen.password_input = sensitive("secretpassword");
    screen.state = UnlockPhase::Verifying;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("unlock_screen_verifying_state", backend);
}

#[test]
fn unlock_screen_locked_out_state() {
    let mut screen = UnlockScreen::default();
    screen.mode = UnlockMode::Password;
    screen.failed_attempts = 5;
    screen.state = UnlockPhase::LockedOut {
        // `as_secs()` floors fractional seconds after render setup; +31s snapshots as 30s.
        locked_until: Instant::now() + Duration::from_secs(31),
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("unlock_screen_locked_out_state", backend);
}

#[test]
fn unlock_screen_success_state() {
    let mut screen = UnlockScreen::default();
    screen.mode = UnlockMode::Password;
    screen.state = UnlockPhase::Success;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("unlock_screen_success_state", backend);
}
