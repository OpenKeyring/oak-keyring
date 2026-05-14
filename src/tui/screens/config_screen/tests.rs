use super::*;
use crate::commands::result::CommandResult;
use crate::commands::{Command, Message};
use crate::config::AppConfig;
use crate::tui::state::config_state::ConfigTab;
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crossterm::event::{KeyCode, KeyModifiers};
use tokio::sync::mpsc;

fn test_context<'a>(tx: &'a mpsc::Sender<Command>, config: &'a AppConfig) -> ScreenContext<'a> {
    ScreenContext {
        command_tx: tx,
        config,
    }
}

#[test]
fn config_loaded_after_snapshot_restore_preserves_navigation_state() {
    let mut screen = ConfigScreen::new();
    screen
        .state
        .restore_from(crate::tui::state::ConfigRestoreState {
            active_tab: ConfigTab::Security,
            focused_item: 3,
            sub_item_focus: Some(1),
            scroll_offset: 4,
        });

    let (tx, _rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(
        Message::CommandCompleted(CommandResult::ConfigLoaded {
            config: config.clone(),
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.state.active_tab, ConfigTab::Security);
    assert_eq!(screen.state.focused_item, 3);
    assert_eq!(screen.state.sub_item_focus, Some(1));
    assert_eq!(screen.state.scroll_offset, 4);
}

fn make_key(code: KeyCode) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn j_key_moves_focus_down() {
    let mut screen = ConfigScreen::new();
    assert_eq!(screen.state.focused_item, 0);

    let (tx, _rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Char('j'))), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.state.focused_item, 1);
}

#[test]
fn k_key_moves_focus_up() {
    let mut screen = ConfigScreen::new();
    screen.state.focused_item = 3;

    let (tx, _rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Char('k'))), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.state.focused_item, 2);
}

#[test]
fn j_key_at_bottom_boundary_moves_focus_to_footer() {
    let mut screen = ConfigScreen::new();
    // General tab has 7 items (0..6), set to last item
    screen.state.focused_item = 6;

    let (tx, _rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Char('j'))), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    // At bottom boundary, focus moves to footer instead of wrapping
    assert!(screen.state.footer_focus.is_some());
    assert!(matches!(
        screen.state.footer_focus,
        Some(crate::tui::state::config_state::FooterButton::ExitProgram)
    ));
}

#[test]
fn k_key_at_top_boundary_triggers_flash() {
    let mut screen = ConfigScreen::new();
    assert_eq!(screen.state.focused_item, 0);

    let (tx, _rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Char('k'))), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.state.focused_item, 6);
    assert!(screen.state.boundary_flash_at.is_some());
}

#[test]
fn q_key_returns_exit_app() {
    let mut screen = ConfigScreen::new();

    let (tx, _rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Char('q'))), &mut ctx);
    assert!(matches!(result, ScreenResult::ExitApp));
}

#[test]
fn j_key_no_boundary_flash_when_not_at_edge() {
    let mut screen = ConfigScreen::new();
    assert_eq!(screen.state.focused_item, 0);

    let (tx, _rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Char('j'))), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.state.focused_item, 1);
    assert!(screen.state.boundary_flash_at.is_none());
}
