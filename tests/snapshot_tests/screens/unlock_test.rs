use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::tui::screens::unlock::{UnlockMode, UnlockPhase, UnlockScreen};
use oak_keyring::tui::traits::screen::Screen;

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
    screen.password_input = "secretpassword".to_string();
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
    screen.password_input = "wrong".to_string();
    screen.state = UnlockPhase::Failed;
    screen.error_message = Some("Wrong password. Please try again.".to_string());
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("unlock_screen_failed_state", backend);
}
