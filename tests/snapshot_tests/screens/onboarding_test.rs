use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::commands::types::{FailedItem, ImportPreview, ReviewItem};
use oak_keyring::tui::screens::import_export::ImportFocus;
use oak_keyring::tui::screens::onboarding::{
    OnboardingPath, OnboardingScreen, OnboardingStep, RecoveryFocus,
};
use oak_keyring::tui::traits::screen::Screen;
use oak_keyring::types::recovery_words::RecoveryWords;
use oak_keyring::types::sensitive::SensitiveInput;

use crate::support::snapshot_locale;

fn sensitive(s: &str) -> SensitiveInput {
    let mut input = SensitiveInput::new();
    for c in s.chars() {
        input.push_char(c);
    }
    input
}

fn render_screen(screen: &OnboardingScreen, width: u16, height: u16) -> TestBackend {
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
fn onboarding_welcome_create_new() {
    let _locale = snapshot_locale();
    let screen = OnboardingScreen {
        current_step: OnboardingStep::Welcome,
        welcome_selected: 0,
        ..Default::default()
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("onboarding_welcome_create_new", backend);
}

#[test]
fn onboarding_welcome_restore() {
    let _locale = snapshot_locale();
    let screen = OnboardingScreen {
        current_step: OnboardingStep::Welcome,
        welcome_selected: 1,
        ..Default::default()
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("onboarding_welcome_restore", backend);
}

#[test]
fn onboarding_recovery_display_default() {
    let _locale = snapshot_locale();
    let words = RecoveryWords::new(vec!["abandon".to_string(); 24]).unwrap();
    let screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryDisplay,
        selected_path: Some(OnboardingPath::CreateNew),
        recovery_words: Some(words),
        recovery_focus: RecoveryFocus::CopyButton,
        ..Default::default()
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("onboarding_recovery_display_default", backend);
}

#[test]
fn onboarding_recovery_display_confirmed() {
    let _locale = snapshot_locale();
    let words = RecoveryWords::new(vec!["abandon".to_string(); 24]).unwrap();
    let screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryDisplay,
        selected_path: Some(OnboardingPath::CreateNew),
        recovery_words: Some(words),
        recovery_focus: RecoveryFocus::ConfirmCheckbox,
        recovery_confirmed: true,
        ..Default::default()
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("onboarding_recovery_display_confirmed", backend);
}

#[test]
fn onboarding_recovery_verify_partial() {
    let _locale = snapshot_locale();
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [2, 5, 12, 21],
        },
        selected_path: Some(OnboardingPath::CreateNew),
        verify_positions: [2, 5, 12, 21],
        verify_focus_index: 2,
        ..Default::default()
    };
    screen.verify_inputs[0] = sensitive("abandon");
    screen.verify_inputs[1] = sensitive("wrongword");
    screen.verify_errors[1] = true;
    screen.verify_inputs[2] = sensitive("ab");
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("onboarding_recovery_verify_partial", backend);
}

#[test]
fn onboarding_recovery_input() {
    let _locale = snapshot_locale();
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryInput,
        selected_path: Some(OnboardingPath::Restore),
        ..Default::default()
    };
    screen.recovery_grid.words[0] = "abandon".to_string();
    screen.recovery_grid.words[1] = "ability".to_string();
    screen.recovery_grid.focused_index = 2;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("onboarding_recovery_input", backend);
}

#[test]
fn onboarding_security_advisory() {
    let _locale = snapshot_locale();
    let screen = OnboardingScreen {
        current_step: OnboardingStep::SecurityAdvisory,
        selected_path: Some(OnboardingPath::Restore),
        ..Default::default()
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("onboarding_security_advisory", backend);
}

#[test]
fn onboarding_import_source() {
    let _locale = snapshot_locale();
    let screen = OnboardingScreen {
        current_step: OnboardingStep::ImportSource,
        selected_path: Some(OnboardingPath::Import),
        import_focus: ImportFocus::FilePath,
        import_file_path: "/home/user/passwords.kdbx".to_string(),
        selected_source_idx: 0,
        ..Default::default()
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("onboarding_import_source", backend);
}

#[test]
fn onboarding_import_preview() {
    let _locale = snapshot_locale();
    let preview = ImportPreview {
        importable: 42,
        needs_review: 2,
        failed: 1,
        review_items: vec![
            ReviewItem {
                name: "Work Login".to_string(),
                reason: "Weak password strength".to_string(),
            },
            ReviewItem {
                name: "Bank Account".to_string(),
                reason: "URL field is missing".to_string(),
            },
        ],
        failed_items: vec![FailedItem {
            name: "Corrupt Entry".to_string(),
            reason: "Invalid UTF-8 decryption payload".to_string(),
        }],
        csv_headers: vec![],
    };
    let screen = OnboardingScreen {
        current_step: OnboardingStep::ImportPreview,
        selected_path: Some(OnboardingPath::Import),
        import_preview: Some(preview),
        import_as_notes: true,
        import_preview_checkbox_focused: true,
        ..Default::default()
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("onboarding_import_preview", backend);
}

#[test]
fn onboarding_set_password() {
    let _locale = snapshot_locale();
    let screen = OnboardingScreen {
        current_step: OnboardingStep::SetPassword,
        selected_path: Some(OnboardingPath::CreateNew),
        ..Default::default()
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("onboarding_set_password", backend);
}
