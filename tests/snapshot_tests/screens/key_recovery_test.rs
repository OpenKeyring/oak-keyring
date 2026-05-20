use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::tui::screens::key_recovery::{KeyRecoveryOrigin, KeyRecoveryScreen};
use oak_keyring::tui::traits::screen::Screen;

use crate::support::snapshot_locale;

fn render_screen(screen: &KeyRecoveryScreen, width: u16, height: u16) -> TestBackend {
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
fn key_recovery_startup_empty() {
    let _locale = snapshot_locale();
    let screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::StartupDbOnly);
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("key_recovery_startup_empty", backend);
}

#[test]
fn key_recovery_onboarding_partial() {
    let _locale = snapshot_locale();
    let mut screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::OnboardingRestore);
    screen.words.words[0] = "abandon".to_string();
    screen.words.words[1] = "ability".to_string();
    screen.words.words[2] = "able".to_string();
    screen.words.focused_index = 3;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("key_recovery_onboarding_partial", backend);
}

#[test]
fn key_recovery_with_error() {
    let _locale = snapshot_locale();
    let mut screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::StartupDbOnly);
    screen.error = Some("Invalid BIP39 mnemonic checksum".to_string());
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("key_recovery_with_error", backend);
}
