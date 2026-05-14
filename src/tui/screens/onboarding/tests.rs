use super::types::{OnboardingStep, RecoveryFocus};
use super::*;
use crate::commands::result::CommandResult;
use crate::commands::types::Screen;
use crate::commands::Command;
use crate::tui::traits::screen::{ScreenContext, ScreenResult};
use crate::types::sensitive::SensitiveInput;
use crossterm::event::{KeyCode, KeyEvent};

#[test]
fn onboarding_welcome_defaults() {
    let screen = OnboardingScreen::default();
    assert!(screen.selected_path.is_none());
    assert_eq!(screen.current_step, OnboardingStep::Welcome);
    assert_eq!(screen.welcome_selected, 0);
    assert_eq!(screen.language_index, 0);
    assert!(screen.error.is_none());
    assert!(!screen.recovery_confirmed);
    assert!(screen.recovery_words.is_empty());
    assert!(screen.verify_inputs.iter().all(|s| s.is_empty()));
    assert!(screen.verify_errors.iter().all(|&e| !e));
}

#[test]
fn onboarding_create_path_steps() {
    let screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        ..Default::default()
    };
    assert_eq!(screen.total_steps(), 4);
}

#[test]
fn onboarding_restore_path_steps() {
    let screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Restore),
        ..Default::default()
    };
    assert_eq!(screen.total_steps(), 3);
}

#[test]
fn onboarding_import_path_steps() {
    let screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Import),
        ..Default::default()
    };
    assert_eq!(screen.total_steps(), 6);
}

#[test]
fn onboarding_no_path_steps() {
    let screen = OnboardingScreen::default();
    assert_eq!(screen.total_steps(), 1);
}

#[test]
fn onboarding_welcome_default_selected_is_first() {
    let screen = OnboardingScreen::default();
    assert_eq!(screen.welcome_selected, 0);
}

#[test]
fn onboarding_welcome_enter_selects_create() {
    let mut screen = OnboardingScreen::default();
    // Default selection is 0 (CreateNew), pressing Enter should select it
    let result = screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.selected_path, Some(OnboardingPath::CreateNew));
    assert_eq!(screen.current_step, OnboardingStep::RecoveryDisplay);
}

#[test]
fn onboarding_welcome_down_then_enter_selects_restore() {
    let mut screen = OnboardingScreen::default();
    // Press Down to move to index 1 (Restore)
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.welcome_selected, 1);

    let result = screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert!(matches!(
        result,
        ScreenResult::NavigateTo(Screen::KeyRecovery)
    ));
    assert_eq!(screen.selected_path, Some(OnboardingPath::Restore));
}

#[test]
fn onboarding_welcome_down_twice_then_enter_selects_import() {
    let mut screen = OnboardingScreen::default();
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.welcome_selected, 2);

    let result = screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.selected_path, Some(OnboardingPath::Import));
    assert_eq!(screen.current_step, OnboardingStep::ImportSource);
}

#[test]
fn onboarding_welcome_down_wraps_around() {
    let mut screen = OnboardingScreen::default();

    // Down three times from 0 should wrap back to 0
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.welcome_selected, 1);
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.welcome_selected, 2);
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.welcome_selected, 0);
}

#[test]
fn onboarding_welcome_up_wraps_around() {
    let mut screen = OnboardingScreen::default();
    // Up from 0 should wrap to 2
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Up, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.welcome_selected, 2);
}

#[test]
fn onboarding_welcome_tab_moves_down() {
    let mut screen = OnboardingScreen::default();
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Tab, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.welcome_selected, 1);
}

#[test]
fn onboarding_welcome_backtab_moves_up() {
    let mut screen = OnboardingScreen::default();
    screen.welcome_selected = 2;
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::BackTab, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.welcome_selected, 1);
}

#[test]
fn onboarding_welcome_esc_exits() {
    let mut screen = OnboardingScreen::default();
    let result = screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert!(matches!(result, ScreenResult::ExitApp));
}

#[test]
fn onboarding_welcome_language_cycling() {
    let mut screen = OnboardingScreen::default();
    assert_eq!(screen.language_index, 0);

    // Press 'L' to cycle to next language
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Char('L'), crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.language_index, 1);

    // Press 'l' (lowercase) to cycle again
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Char('l'), crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.language_index, 2);

    // Press 'L' to wrap around to auto
    screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Char('L'), crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.language_index, 0);
}

#[test]
fn onboarding_recovery_display_space_toggles_checkbox() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        recovery_focus: RecoveryFocus::ConfirmCheckbox,
        ..Default::default()
    };

    assert!(!screen.recovery_confirmed);

    // Space toggles checkbox when ConfirmCheckbox is focused
    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Char(' '), crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert!(screen.recovery_confirmed);

    // Space again to unconfirm
    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Char(' '), crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert!(!screen.recovery_confirmed);
}

#[test]
fn onboarding_recovery_display_space_ignored_on_buttons() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        recovery_focus: RecoveryFocus::CopyButton,
        ..Default::default()
    };

    // Space should NOT toggle checkbox when copy button is focused
    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Char(' '), crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert!(!screen.recovery_confirmed);
}

#[test]
fn onboarding_recovery_display_enter_without_confirm() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        recovery_confirmed: false,
        recovery_focus: RecoveryFocus::ConfirmCheckbox,
        ..Default::default()
    };

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    // Should NOT advance without confirmation
    assert_eq!(screen.current_step, OnboardingStep::RecoveryDisplay);
}

#[test]
fn onboarding_recovery_display_enter_with_confirm() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        recovery_confirmed: true,
        recovery_focus: RecoveryFocus::ConfirmCheckbox,
        recovery_words: vec!["abandon".to_string(); 24],
        ..Default::default()
    };

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert!(matches!(
        screen.current_step,
        OnboardingStep::RecoveryVerify { .. }
    ));
    // Should have 4 positions sorted
    let sorted = {
        let mut p = screen.verify_positions;
        p.sort();
        p
    };
    assert_eq!(screen.verify_positions, sorted);
}

#[test]
fn onboarding_recovery_display_tab_cycles_focus() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        ..Default::default()
    };

    assert_eq!(screen.recovery_focus, RecoveryFocus::CopyButton);

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Tab, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.recovery_focus, RecoveryFocus::RegenerateButton);

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Tab, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.recovery_focus, RecoveryFocus::ConfirmCheckbox);

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Tab, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.recovery_focus, RecoveryFocus::CopyButton);
}

#[test]
fn onboarding_recovery_display_backtab_cycles_focus_reverse() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        ..Default::default()
    };

    assert_eq!(screen.recovery_focus, RecoveryFocus::CopyButton);

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::BackTab, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.recovery_focus, RecoveryFocus::ConfirmCheckbox);

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::BackTab, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.recovery_focus, RecoveryFocus::RegenerateButton);

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::BackTab, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.recovery_focus, RecoveryFocus::CopyButton);
}

#[test]
fn onboarding_recovery_display_copy_button_sets_copied_flag() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        recovery_focus: RecoveryFocus::CopyButton,
        recovery_words: vec!["abandon".to_string(); 24],
        ..Default::default()
    };

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );

    assert!(
        screen.clipboard_copied,
        "clipboard_copied should be set after copy"
    );
}

#[test]
fn onboarding_recovery_display_copy_skipped_when_empty() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        recovery_focus: RecoveryFocus::CopyButton,
        recovery_words: vec![],
        ..Default::default()
    };

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );

    assert!(
        !screen.clipboard_copied,
        "clipboard_copied should NOT be set when words are empty"
    );
}

#[test]
fn onboarding_recovery_display_regenerate_generates_new_words_locally() {
    let original_words: Vec<String> = (0..24).map(|i| format!("word{}", i)).collect();
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        recovery_focus: RecoveryFocus::RegenerateButton,
        recovery_words: original_words.clone(),
        recovery_confirmed: true,
        clipboard_copied: true,
        ..Default::default()
    };

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );

    // Confirm and clipboard state should be reset immediately
    assert!(!screen.recovery_confirmed);
    assert!(!screen.clipboard_copied);
    // Words are regenerated locally — new BIP39 mnemonic replaces the old ones
    assert_ne!(screen.recovery_words, original_words);
    assert_eq!(screen.recovery_words.len(), 24);
}

#[test]
fn onboarding_recovery_display_regenerate_sends_command_even_with_empty_words() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        recovery_focus: RecoveryFocus::RegenerateButton,
        recovery_words: vec![],
        ..Default::default()
    };

    let result = screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );

    // Should regenerate locally — words are no longer empty
    assert!(matches!(result, ScreenResult::Continue));
    assert!(
        !screen.recovery_words.is_empty(),
        "recovery_words should be populated after regenerate"
    );
    assert_eq!(
        screen.recovery_words.len(),
        24,
        "should generate exactly 24 words"
    );
}

#[test]
fn onboarding_recovery_display_esc_goes_back() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        ..Default::default()
    };

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.current_step, OnboardingStep::Welcome);
}

#[test]
fn onboarding_recovery_display_right_arrow_cycles() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        ..Default::default()
    };

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Right, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.recovery_focus, RecoveryFocus::RegenerateButton);
}

#[test]
fn onboarding_recovery_display_left_arrow_cycles() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        ..Default::default()
    };

    screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Left, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert_eq!(screen.recovery_focus, RecoveryFocus::ConfirmCheckbox);
}

#[test]
fn onboarding_recovery_display_defaults() {
    let screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        ..Default::default()
    };
    assert_eq!(screen.recovery_focus, RecoveryFocus::CopyButton);
    assert!(!screen.clipboard_copied);
    assert_eq!(screen.clipboard_clear_seconds, 30);
}

#[test]
fn onboarding_step_number_create_path() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        ..Default::default()
    };

    screen.current_step = OnboardingStep::Welcome;
    assert_eq!(screen.current_step_number(), 1);

    screen.current_step = OnboardingStep::RecoveryDisplay;
    assert_eq!(screen.current_step_number(), 2);

    screen.current_step = OnboardingStep::RecoveryVerify {
        positions: [0, 5, 10, 15],
    };
    assert_eq!(screen.current_step_number(), 3);

    screen.current_step = OnboardingStep::SetPassword;
    assert_eq!(screen.current_step_number(), 4);
}

#[test]
fn onboarding_step_number_restore_path() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Restore),
        ..Default::default()
    };

    screen.current_step = OnboardingStep::Welcome;
    assert_eq!(screen.current_step_number(), 1);

    screen.current_step = OnboardingStep::RecoveryInput;
    assert_eq!(screen.current_step_number(), 2);

    screen.current_step = OnboardingStep::SecurityAdvisory;
    assert_eq!(screen.current_step_number(), 3);
}

#[test]
fn onboarding_step_number_import_path() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Import),
        ..Default::default()
    };

    screen.current_step = OnboardingStep::Welcome;
    assert_eq!(screen.current_step_number(), 1);

    screen.current_step = OnboardingStep::ImportSource;
    assert_eq!(screen.current_step_number(), 2);

    screen.current_step = OnboardingStep::ImportPreview;
    assert_eq!(screen.current_step_number(), 3);

    screen.current_step = OnboardingStep::RecoveryDisplay;
    assert_eq!(screen.current_step_number(), 4);

    screen.current_step = OnboardingStep::RecoveryVerify {
        positions: [0, 1, 2, 3],
    };
    assert_eq!(screen.current_step_number(), 5);

    screen.current_step = OnboardingStep::SetPassword;
    assert_eq!(screen.current_step_number(), 6);
}

#[test]
fn onboarding_set_password_navigates() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::SetPassword,
        ..Default::default()
    };

    let result = screen.handle_set_password_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );
    assert!(matches!(
        result,
        ScreenResult::NavigateTo(Screen::SetNewMasterPassword)
    ));
}

#[test]
fn onboarding_security_advisory_enter() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Restore),
        current_step: OnboardingStep::SecurityAdvisory,
        ..Default::default()
    };

    let result = screen.handle_security_advisory_key(KeyEvent::new(
        KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.current_step, OnboardingStep::SetPassword);
}

#[test]
fn onboarding_import_source_enter() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Import),
        current_step: OnboardingStep::ImportSource,
        ..Default::default()
    };
    let mut ctx = dummy_ctx();

    let result = screen.handle_import_source_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut ctx,
    );
    // Import source Enter now validates the file (requires file path),
    // so without a file path it shows an error instead of navigating.
    assert!(matches!(result, ScreenResult::Continue));
}

#[test]
fn onboarding_import_preview_enter() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Import),
        current_step: OnboardingStep::ImportPreview,
        ..Default::default()
    };
    let mut ctx = dummy_ctx();

    let result = screen.handle_import_preview_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut ctx,
    );
    assert!(matches!(result, ScreenResult::Continue));
    // ImportPreview Enter sends ExecuteImport command, stays on ImportPreview
    // until ImportCompleted result is received.
}

#[test]
fn onboarding_import_preview_enter_toggles_checkbox_when_focused() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Import),
        current_step: OnboardingStep::ImportPreview,
        import_preview_checkbox_focused: true,
        ..Default::default()
    };
    let mut ctx = dummy_ctx();

    assert!(!screen.import_as_notes);

    // Enter toggles checkbox when focused (does NOT trigger import)
    screen.handle_import_preview_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut ctx,
    );
    assert!(screen.import_as_notes);

    // Enter again to toggle back
    screen.handle_import_preview_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut ctx,
    );
    assert!(!screen.import_as_notes);
}

#[test]
fn onboarding_import_preview_space_toggles_checkbox_when_focused() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Import),
        current_step: OnboardingStep::ImportPreview,
        import_preview_checkbox_focused: true,
        ..Default::default()
    };
    let mut ctx = dummy_ctx();

    assert!(!screen.import_as_notes);

    // Space toggles checkbox when focused
    screen.handle_import_preview_key(
        KeyEvent::new(KeyCode::Char(' '), crossterm::event::KeyModifiers::NONE),
        &mut ctx,
    );
    assert!(screen.import_as_notes);
}

#[test]
fn onboarding_import_preview_space_ignored_when_checkbox_not_focused() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Import),
        current_step: OnboardingStep::ImportPreview,
        import_preview_checkbox_focused: false,
        ..Default::default()
    };
    let mut ctx = dummy_ctx();

    assert!(!screen.import_as_notes);

    // Space should NOT toggle checkbox when it is not focused
    screen.handle_import_preview_key(
        KeyEvent::new(KeyCode::Char(' '), crossterm::event::KeyModifiers::NONE),
        &mut ctx,
    );
    assert!(!screen.import_as_notes);
}

#[test]
fn onboarding_import_preview_tab_toggles_focus() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Import),
        current_step: OnboardingStep::ImportPreview,
        ..Default::default()
    };
    let mut ctx = dummy_ctx();

    assert!(!screen.import_preview_checkbox_focused);

    // Tab toggles checkbox focus
    screen.handle_import_preview_key(
        KeyEvent::new(KeyCode::Tab, crossterm::event::KeyModifiers::NONE),
        &mut ctx,
    );
    assert!(screen.import_preview_checkbox_focused);

    // Tab again toggles back
    screen.handle_import_preview_key(
        KeyEvent::new(KeyCode::Tab, crossterm::event::KeyModifiers::NONE),
        &mut ctx,
    );
    assert!(!screen.import_preview_checkbox_focused);
}

#[test]
fn onboarding_import_preview_backtab_toggles_focus() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Import),
        current_step: OnboardingStep::ImportPreview,
        import_preview_checkbox_focused: false,
        ..Default::default()
    };
    let mut ctx = dummy_ctx();

    // BackTab also toggles checkbox focus
    screen.handle_import_preview_key(
        KeyEvent::new(KeyCode::BackTab, crossterm::event::KeyModifiers::NONE),
        &mut ctx,
    );
    assert!(screen.import_preview_checkbox_focused);
}

#[test]
fn onboarding_command_result_error() {
    let mut screen = OnboardingScreen::default();
    let result = screen.handle_command_result(CommandResult::Error {
        code: crate::errors::ErrorCode::VaultRecordNotFound,
        context: crate::errors::ErrorContext::new(),
        message_key: "vault.not_found",
        fallback: "Vault not found".to_string(),
    });

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.error, Some("Vault not found".to_string()));
}

#[test]
fn onboarding_generate_verify_positions() {
    let mut screen = OnboardingScreen::default();
    screen.generate_verify_positions();

    // Should have 4 unique positions, all in 0..24
    assert_eq!(screen.verify_positions.len(), 4);
    let mut sorted = screen.verify_positions;
    sorted.sort();
    assert_eq!(screen.verify_positions, sorted); // sorted

    // All unique
    let unique: std::collections::HashSet<usize> =
        screen.verify_positions.iter().copied().collect();
    assert_eq!(unique.len(), 4);

    // All in range
    for &pos in &screen.verify_positions {
        assert!(pos < 24);
    }

    // Inputs and errors should be reset
    assert!(screen.verify_inputs.iter().all(|s| s.is_empty()));
    assert!(screen.verify_errors.iter().all(|&e| !e));
}

// ── RecoveryVerify Tab navigation tests ────────────────────────────────

#[test]
fn onboarding_verify_default_focus_is_first_box() {
    let screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        verify_positions: [0, 5, 10, 15],
        ..Default::default()
    };
    assert_eq!(screen.verify_focus_index, 0);
}

#[test]
fn onboarding_verify_tab_advances_focus() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        verify_positions: [0, 5, 10, 15],
        ..Default::default()
    };

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 1);

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 2);

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 3);
}

#[test]
fn onboarding_verify_tab_clamps_at_last_box() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        verify_focus_index: 3,
        verify_positions: [0, 5, 10, 15],
        ..Default::default()
    };

    // Tab on last box when not all filled should clamp
    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 3);
    assert_eq!(
        screen.current_step,
        OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        }
    );
}

#[test]
fn onboarding_verify_tab_on_last_box_submits_when_all_filled() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        verify_focus_index: 3,
        verify_positions: [0, 5, 10, 15],
        recovery_words: vec![
            "abandon".to_string(),
            "ability".to_string(),
            "able".to_string(),
            "about".to_string(),
            "above".to_string(),
            "absent".to_string(), // index 5
            "absorb".to_string(),
            "abstract".to_string(),
            "absurd".to_string(),
            "abundance".to_string(),
            "academy".to_string(), // index 10
            "accept".to_string(),
            "access".to_string(),
            "accident".to_string(),
            "account".to_string(),
            "accuse".to_string(), // index 15
            "achieve".to_string(),
            "acid".to_string(),
            "acoustic".to_string(),
            "acquire".to_string(),
            "across".to_string(),
            "act".to_string(),
            "action".to_string(),
            "actor".to_string(),
        ],
        verify_inputs: [
            SensitiveInput::from("abandon".to_string()), // matches pos 0
            SensitiveInput::from("absent".to_string()),  // matches pos 5
            SensitiveInput::from("academy".to_string()), // matches pos 10
            SensitiveInput::from("accuse".to_string()),  // matches pos 15
        ],
        ..Default::default()
    };

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.current_step, OnboardingStep::SetPassword);
}

#[test]
fn onboarding_verify_shifttab_goes_back() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        verify_focus_index: 3,
        verify_positions: [0, 5, 10, 15],
        ..Default::default()
    };

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::BackTab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 2);

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::BackTab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 1);

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::BackTab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 0);
}

#[test]
fn onboarding_verify_shifttab_clamps_at_first_box() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        verify_focus_index: 0,
        ..Default::default()
    };

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::BackTab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 0);
}

#[test]
fn onboarding_verify_typing_affects_focused_box() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        verify_focus_index: 2,
        verify_positions: [0, 5, 10, 15],
        ..Default::default()
    };

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Char('h'),
        crossterm::event::KeyModifiers::NONE,
    ));
    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Char('e'),
        crossterm::event::KeyModifiers::NONE,
    ));
    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Char('l'),
        crossterm::event::KeyModifiers::NONE,
    ));
    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Char('l'),
        crossterm::event::KeyModifiers::NONE,
    ));
    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Char('o'),
        crossterm::event::KeyModifiers::NONE,
    ));

    assert!(screen.verify_inputs[0].is_empty());
    assert!(screen.verify_inputs[1].is_empty());
    assert_eq!(screen.verify_inputs[2].expose(|s| s.to_string()), "hello");
    assert!(screen.verify_inputs[3].is_empty());
}

#[test]
fn onboarding_verify_backspace_affects_focused_box() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        verify_focus_index: 1,
        verify_positions: [0, 5, 10, 15],
        verify_inputs: [
            SensitiveInput::from("abandon".to_string()),
            SensitiveInput::from("hello".to_string()),
            SensitiveInput::new(),
            SensitiveInput::new(),
        ],
        ..Default::default()
    };

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Backspace,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_inputs[0].expose(|s| s.to_string()), "abandon");
    assert_eq!(screen.verify_inputs[1].expose(|s| s.to_string()), "hell");
    assert!(screen.verify_inputs[2].is_empty());
}

#[test]
fn onboarding_verify_enter_validates() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        verify_positions: [0, 5, 10, 15],
        recovery_words: vec![
            "abandon".to_string(),
            "ability".to_string(),
            "able".to_string(),
            "about".to_string(),
            "above".to_string(),
            "absent".to_string(),
            "absorb".to_string(),
            "abstract".to_string(),
            "absurd".to_string(),
            "abundance".to_string(),
            "academy".to_string(),
            "accept".to_string(),
            "access".to_string(),
            "accident".to_string(),
            "account".to_string(),
            "accuse".to_string(),
            "achieve".to_string(),
            "acid".to_string(),
            "acoustic".to_string(),
            "acquire".to_string(),
            "across".to_string(),
            "act".to_string(),
            "action".to_string(),
            "actor".to_string(),
        ],
        verify_inputs: [
            SensitiveInput::from("abandon".to_string()),
            SensitiveInput::from("WRONG".to_string()),
            SensitiveInput::from("academy".to_string()),
            SensitiveInput::from("accuse".to_string()),
        ],
        ..Default::default()
    };

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));
    // Should stay on RecoveryVerify with errors marked
    assert!(matches!(
        screen.current_step,
        OnboardingStep::RecoveryVerify { .. }
    ));
    assert!(!screen.verify_errors[0]); // correct
    assert!(screen.verify_errors[1]); // wrong
    assert!(!screen.verify_errors[2]); // correct
    assert!(!screen.verify_errors[3]); // correct
}

#[test]
fn onboarding_verify_esc_goes_back_to_recovery_display() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        ..Default::default()
    };

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.current_step, OnboardingStep::RecoveryDisplay);
}

#[test]
fn on_unmount_zeroizes_sensitive_data() {
    use crate::tui::traits::screen::Screen;

    let mut screen = OnboardingScreen::default();
    screen.recovery_words = vec!["secret".to_string(); 24];
    screen.verify_inputs[0] = SensitiveInput::from("secret".to_string());
    screen.verify_positions = [1, 2, 3, 4];
    for word in &mut screen.recovery_grid.words {
        word.push_str("secret");
    }

    screen.on_unmount();

    assert!(screen.recovery_words.is_empty());
    assert!(screen.verify_inputs.iter().all(|s| s.is_empty()));
    assert!(screen.recovery_grid.words.iter().all(|w| w.is_empty()));
    assert_eq!(screen.verify_positions, [0, 0, 0, 0]);
}

/// Helper to create a dummy ScreenContext for tests.
/// The command_tx is a buffered channel that discards messages.
#[allow(static_mut_refs)]
fn dummy_ctx() -> ScreenContext<'static> {
    // We cannot easily construct ScreenContext in unit tests,
    // so we leak the channel to get 'static lifetime.
    static ONCE: std::sync::Once = std::sync::Once::new();
    static mut TX: Option<tokio::sync::mpsc::Sender<Command>> = None;

    ONCE.call_once(|| {
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        unsafe { TX = Some(tx) };
    });

    let tx = unsafe { TX.as_ref().unwrap() };
    static DUMMY_CONFIG: std::sync::OnceLock<crate::config::AppConfig> = std::sync::OnceLock::new();
    let config = DUMMY_CONFIG.get_or_init(crate::config::AppConfig::default);

    ScreenContext {
        command_tx: tx,
        config,
    }
}
