use super::types::{OnboardingStep, RecoveryFocus};
use super::*;
use crate::commands::result::CommandResult;
use crate::commands::types::Screen;
use crate::commands::Command;
use crate::tui::traits::screen::Screen as ScreenTrait;
use crate::tui::traits::screen::{ScreenContext, ScreenResult};
use crate::types::sensitive::SensitiveInput;
use crate::types::RecoveryWords;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::{Backend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::Terminal;

fn recovery_words_fixture() -> RecoveryWords {
    RecoveryWords::new(vec!["abandon".to_string(); 24]).unwrap()
}

fn indexed_recovery_words() -> RecoveryWords {
    RecoveryWords::new(vec![
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
    ])
    .unwrap()
}

fn render_onboarding(screen: &OnboardingScreen, width: u16, height: u16) -> String {
    format!("{:?}", render_onboarding_buffer(screen, width, height))
}

fn render_onboarding_buffer(screen: &OnboardingScreen, width: u16, height: u16) -> Buffer {
    let _guard = crate::tui::i18n::LocaleGuard::en();
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            screen.view(frame, frame.area());
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn render_onboarding_cursor_position(
    screen: &OnboardingScreen,
    width: u16,
    height: u16,
) -> Position {
    let _guard = crate::tui::i18n::LocaleGuard::en();
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            screen.view(frame, frame.area());
        })
        .unwrap();
    terminal.backend_mut().get_cursor_position().unwrap()
}

fn click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }
}

fn mouse_move(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Moved,
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }
}

#[test]
fn onboarding_welcome_defaults() {
    let screen = OnboardingScreen::default();
    assert!(screen.selected_path.is_none());
    assert_eq!(screen.current_step, OnboardingStep::Welcome);
    assert_eq!(screen.welcome_selected, 0);
    assert_eq!(screen.language_index, 0);
    assert!(screen.error.is_none());
    assert!(!screen.recovery_confirmed);
    assert!(screen.recovery_words.is_none());
    assert!(screen.verify_inputs.iter().all(|s| s.is_empty()));
    assert!(screen.verify_errors.iter().all(|&e| !e));
}

#[test]
fn onboarding_intro_motion_is_one_shot() {
    let mut screen = OnboardingScreen::default();

    assert_eq!(
        screen.take_intro_motion(),
        Some(crate::tui::state::animation::EffectKind::OnboardingIntro)
    );
    assert_eq!(screen.take_intro_motion(), None);
}

#[test]
fn onboarding_welcome_enter_selects_create_and_requests_forward_motion() {
    let mut screen = OnboardingScreen::default();

    let result = screen.handle_welcome_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.selected_path, Some(OnboardingPath::CreateNew));
    assert_eq!(screen.current_step, OnboardingStep::RecoveryDisplay);
    assert_eq!(
        screen.take_pending_motion(),
        Some(crate::tui::state::animation::EffectKind::OnboardingForward)
    );
}

#[test]
fn onboarding_recovery_display_esc_requests_back_motion() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        recovery_words: Some(recovery_words_fixture()),
        ..Default::default()
    };

    let result = screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.current_step, OnboardingStep::Welcome);
    assert_eq!(
        screen.take_pending_motion(),
        Some(crate::tui::state::animation::EffectKind::OnboardingBack)
    );
}

#[test]
fn onboarding_recovery_key_unlocked_on_current_step_does_not_request_motion() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::Restore),
        current_step: OnboardingStep::SecurityAdvisory,
        pending_motion: None,
        ..Default::default()
    };

    let result = screen.handle_command_result(CommandResult::RecoveryKeyUnlocked);

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.current_step, OnboardingStep::SecurityAdvisory);
    assert_eq!(screen.take_pending_motion(), None);
}

#[test]
fn generate_recovery_words_stores_secure_owner() {
    let mut screen = OnboardingScreen::default();
    screen.generate_recovery_words("en");
    let words = screen
        .recovery_words
        .as_ref()
        .expect("words should be generated");
    assert_eq!(words.len(), 24);
}

#[test]
fn on_unmount_drops_generated_recovery_words() {
    let mut screen = OnboardingScreen::default();
    screen.generate_recovery_words("en");
    assert!(screen.recovery_words.is_some());
    screen.on_unmount();
    assert!(screen.recovery_words.is_none());
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
fn onboarding_welcome_mouse_click_selects_restore() {
    let mut screen = OnboardingScreen::default();
    let _ = render_onboarding(&screen, 80, 24);
    let restore_area = screen.welcome_card_areas[1].get();

    let result = screen.handle_mouse(
        click(restore_area.x + 1, restore_area.y + 1),
        &mut dummy_ctx(),
    );

    assert!(matches!(
        result,
        ScreenResult::NavigateTo(Screen::KeyRecovery)
    ));
    assert_eq!(screen.welcome_selected, 1);
    assert_eq!(screen.selected_path, Some(OnboardingPath::Restore));
}

#[test]
fn onboarding_welcome_mouse_hover_updates_focus() {
    let mut screen = OnboardingScreen::default();
    let _ = render_onboarding(&screen, 80, 24);
    assert_eq!(screen.welcome_selected, 0);

    let import_area = screen.welcome_card_areas[2].get();
    let result = screen.handle_mouse(
        mouse_move(import_area.x + 1, import_area.y + 1),
        &mut dummy_ctx(),
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.welcome_selected, 2);
    // Hover should NOT select a path
    assert!(screen.selected_path.is_none());
}

#[test]
fn onboarding_welcome_mouse_hover_noop_outside_cards() {
    let mut screen = OnboardingScreen::default();
    let _ = render_onboarding(&screen, 80, 24);
    assert_eq!(screen.welcome_selected, 0);

    // Move mouse to top-left corner, outside any card
    let result = screen.handle_mouse(mouse_move(0, 0), &mut dummy_ctx());

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.welcome_selected, 0);
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
    let mut screen = OnboardingScreen {
        welcome_selected: 2,
        ..Default::default()
    };
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
        recovery_words: Some(recovery_words_fixture()),
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
    assert_eq!(screen.recovery_focus, RecoveryFocus::LearnMoreToggle);

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
    assert_eq!(screen.recovery_focus, RecoveryFocus::LearnMoreToggle);

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
        recovery_words: Some(recovery_words_fixture()),
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
        recovery_words: None,
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
    let original_first_word = "word0".to_string();
    let original_words = (0..24).map(|i| format!("word{i}")).collect::<Vec<_>>();
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        recovery_focus: RecoveryFocus::RegenerateButton,
        recovery_words: Some(RecoveryWords::new(original_words).unwrap()),
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
    let words = screen
        .recovery_words
        .as_ref()
        .expect("words should be regenerated");
    assert_ne!(words.word(0), Some(original_first_word.as_str()));
    assert_eq!(words.len(), 24);
}

#[test]
fn onboarding_recovery_display_regenerate_sends_command_even_with_empty_words() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryDisplay,
        recovery_focus: RecoveryFocus::RegenerateButton,
        recovery_words: None,
        ..Default::default()
    };

    let result = screen.handle_recovery_display_key(
        KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
        &mut dummy_ctx(),
    );

    // Should regenerate locally — words are no longer empty
    assert!(matches!(result, ScreenResult::Continue));
    assert!(
        screen.recovery_words.is_some(),
        "recovery_words should be populated after regenerate"
    );
    assert_eq!(
        screen.recovery_words.as_ref().map(RecoveryWords::len),
        Some(24)
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
fn onboarding_recovery_verify_tab_and_arrows_cycle_focus() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 1, 2, 3],
        },
        verify_focus_index: 3,
        ..Default::default()
    };

    let result = screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    ));

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.verify_focus_index, 0);

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Up,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 3);

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 0);
}

#[test]
fn onboarding_recovery_verify_backtab_is_ignored() {
    let mut screen = OnboardingScreen {
        selected_path: Some(OnboardingPath::CreateNew),
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 1, 2, 3],
        },
        verify_focus_index: 2,
        ..Default::default()
    };

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::BackTab,
        crossterm::event::KeyModifiers::NONE,
    ));

    assert_eq!(screen.verify_focus_index, 2);
}

#[test]
fn onboarding_recovery_verify_empty_focused_input_does_not_render_text_cursor() {
    let screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [2, 5, 12, 21],
        },
        selected_path: Some(OnboardingPath::CreateNew),
        verify_positions: [2, 5, 12, 21],
        verify_focus_index: 0,
        ..Default::default()
    };

    let buffer = render_onboarding_buffer(&screen, 80, 24);
    let input_area = screen.verify_box_areas[0].get();

    assert_eq!(
        buffer
            .cell((input_area.x + 1, input_area.y + 1))
            .expect("focused input text cell")
            .symbol(),
        " "
    );
}

#[test]
fn onboarding_recovery_verify_focused_text_input_does_not_append_text_cursor() {
    let screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [2, 5, 12, 21],
        },
        selected_path: Some(OnboardingPath::CreateNew),
        verify_positions: [2, 5, 12, 21],
        verify_focus_index: 1,
        verify_inputs: [
            SensitiveInput::new(),
            SensitiveInput::from("ab".to_string()),
            SensitiveInput::new(),
            SensitiveInput::new(),
        ],
        ..Default::default()
    };

    let buffer = render_onboarding_buffer(&screen, 80, 24);
    let input_area = screen.verify_box_areas[1].get();

    assert_eq!(
        buffer
            .cell((input_area.x + 1, input_area.y + 1))
            .expect("first typed cell")
            .symbol(),
        "a"
    );
    assert_eq!(
        buffer
            .cell((input_area.x + 2, input_area.y + 1))
            .expect("second typed cell")
            .symbol(),
        "b"
    );
    assert_eq!(
        buffer
            .cell((input_area.x + 3, input_area.y + 1))
            .expect("cell after typed input")
            .symbol(),
        " "
    );
}

#[test]
fn onboarding_recovery_verify_empty_focused_input_sets_terminal_cursor() {
    let screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [2, 5, 12, 21],
        },
        selected_path: Some(OnboardingPath::CreateNew),
        verify_positions: [2, 5, 12, 21],
        verify_focus_index: 2,
        ..Default::default()
    };

    let _ = render_onboarding_buffer(&screen, 80, 24);
    let input_area = screen.verify_box_areas[2].get();
    let cursor = render_onboarding_cursor_position(&screen, 80, 24);

    assert_eq!(
        cursor,
        Position {
            x: input_area.x + 1,
            y: input_area.y + 1,
        }
    );
}

#[test]
fn onboarding_recovery_verify_uses_left_labels_and_compact_inputs() {
    let screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [2, 5, 12, 21],
        },
        selected_path: Some(OnboardingPath::CreateNew),
        verify_positions: [2, 5, 12, 21],
        verify_focus_index: 0,
        ..Default::default()
    };

    let buffer = render_onboarding_buffer(&screen, 80, 24);
    let input_area = screen.verify_box_areas[0].get();
    let input_row = input_area.y + 1;
    let label_text = (0..input_area.x)
        .filter_map(|x| buffer.cell((x, input_row)).map(|cell| cell.symbol()))
        .collect::<String>();

    assert!(input_area.width <= 24, "input area was {input_area:?}");
    assert!(
        label_text.contains('3') && !label_text.trim().is_empty(),
        "left label row before input was {label_text:?}"
    );
}

#[test]
fn onboarding_recovery_verify_mouse_hover_focuses_input_box() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        selected_path: Some(OnboardingPath::CreateNew),
        verify_positions: [0, 5, 10, 15],
        verify_focus_index: 0,
        ..Default::default()
    };
    let _ = render_onboarding(&screen, 80, 24);
    let third_box = screen.verify_box_areas[2].get();

    let result = screen.handle_mouse(
        mouse_move(third_box.x + 1, third_box.y + 1),
        &mut dummy_ctx(),
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.verify_focus_index, 2);
}

#[test]
fn onboarding_recovery_verify_mouse_click_focuses_input_box() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        selected_path: Some(OnboardingPath::CreateNew),
        verify_positions: [0, 5, 10, 15],
        verify_focus_index: 0,
        ..Default::default()
    };
    let _ = render_onboarding(&screen, 80, 24);
    let fourth_box = screen.verify_box_areas[3].get();

    let result = screen.handle_mouse(click(fourth_box.x + 1, fourth_box.y + 1), &mut dummy_ctx());

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.verify_focus_index, 3);
}

#[test]
fn onboarding_step_number_create_path() {
    #[allow(clippy::field_reassign_with_default)]
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
    #[allow(clippy::field_reassign_with_default)]
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
    #[allow(clippy::field_reassign_with_default)]
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
    #[allow(clippy::field_reassign_with_default)]
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
    #[allow(clippy::field_reassign_with_default)]
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
    #[allow(clippy::field_reassign_with_default)]
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
    #[allow(clippy::field_reassign_with_default)]
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
fn onboarding_verify_tab_cycles_from_last_box() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        verify_focus_index: 3,
        verify_positions: [0, 5, 10, 15],
        ..Default::default()
    };

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 0);
    assert_eq!(
        screen.current_step,
        OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        }
    );
}

#[test]
fn onboarding_verify_enter_submits_when_all_filled() {
    let mut screen = OnboardingScreen {
        current_step: OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        },
        verify_focus_index: 3,
        verify_positions: [0, 5, 10, 15],
        recovery_words: Some(indexed_recovery_words()),
        verify_inputs: [
            SensitiveInput::from("abandon".to_string()), // matches pos 0
            SensitiveInput::from("absent".to_string()),  // matches pos 5
            SensitiveInput::from("academy".to_string()), // matches pos 10
            SensitiveInput::from("accuse".to_string()),  // matches pos 15
        ],
        ..Default::default()
    };

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.current_step, OnboardingStep::SetPassword);
}

#[test]
fn onboarding_verify_shifttab_is_ignored() {
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
    assert_eq!(screen.verify_focus_index, 3);

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::BackTab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 3);

    screen.handle_recovery_verify_key(KeyEvent::new(
        KeyCode::BackTab,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(screen.verify_focus_index, 3);
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
        recovery_words: Some(indexed_recovery_words()),
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
#[allow(clippy::field_reassign_with_default)]
fn on_unmount_zeroizes_sensitive_data() {
    use crate::tui::traits::screen::Screen;

    let mut screen = OnboardingScreen::default();
    screen.recovery_words = Some(recovery_words_fixture());
    screen.verify_inputs[0] = SensitiveInput::from("secret".to_string());
    screen.verify_positions = [1, 2, 3, 4];
    for word in &mut screen.recovery_grid.words {
        word.push_str("secret");
    }

    screen.on_unmount();

    assert!(screen.recovery_words.is_none());
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
