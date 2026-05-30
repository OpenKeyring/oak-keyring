use super::*;
use crate::commands::result::CommandResult;
use crate::commands::{Command, Message};
use crate::config::{AppConfig, GoogleDriveConfig, ProviderConfig, SyncProvider};
use crate::tui::state::config_state::{ConfigOverlay, ConfigTab, ConfirmButton, GDriveAuthStatus};
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
    screen.state.focused_item = ConfigTab::General.item_count() - 1;

    let (tx, _rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Char('j'))), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    // At bottom boundary, focus moves to footer instead of wrapping
    assert!(matches!(
        screen.state.footer_focus,
        Some(crate::tui::state::config_state::FooterButton::Close)
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
fn q_key_does_not_exit_from_config_screen() {
    let mut screen = ConfigScreen::new();

    let (tx, _rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Char('q'))), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
}

#[test]
fn config_footer_has_only_close_action() {
    let mut screen = ConfigScreen::new();
    screen.state.focused_item = 6;

    let (tx, _rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Char('j'))), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(
        screen.state.footer_focus,
        Some(crate::tui::state::config_state::FooterButton::Close)
    );

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Enter)), &mut ctx);
    assert!(matches!(
        result,
        ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
    ));
}

#[test]
fn esc_unsaved_dialog_can_discard_without_saving() {
    let mut screen = ConfigScreen::new();
    screen.state.has_changes = true;

    let (tx, mut rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Esc)), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));

    match screen.state.overlay {
        Some(ConfigOverlay::UnsavedChanges { focused_button }) => {
            assert_eq!(focused_button, ConfirmButton::Stay);
        }
        ref other => panic!("expected unsaved changes dialog, got {other:?}"),
    }

    screen.update(Message::KeyEvent(make_key(KeyCode::Right)), &mut ctx);
    screen.update(Message::KeyEvent(make_key(KeyCode::Right)), &mut ctx);
    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Enter)), &mut ctx);

    assert!(matches!(
        result,
        ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
    ));
    assert!(rx.try_recv().is_err(), "discard should not save config");
}

#[test]
fn sync_google_drive_down_focuses_authorize_action() {
    let mut screen = ConfigScreen::new();
    screen.state.active_tab = ConfigTab::Sync;
    screen.state.focused_item = 2;
    screen.state.sync.provider = SyncProvider::GoogleDrive;
    screen.state.sync.provider_config = Some(ProviderConfig::GoogleDrive(GoogleDriveConfig {
        root_path: ".oak-keyring/".to_string(),
        ..GoogleDriveConfig::default()
    }));
    screen.state.gdrive_auth_status = GDriveAuthStatus::NotAuthorized;

    let (tx, mut rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    screen.update(Message::KeyEvent(make_key(KeyCode::Down)), &mut ctx);
    assert_eq!(screen.state.focused_item, 3);

    let result = screen.update(Message::KeyEvent(make_key(KeyCode::Enter)), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert!(matches!(
        rx.try_recv(),
        Ok(Command::OAuth2AuthorizeGoogleDrive)
    ));
}

#[test]
fn oauth_success_with_refresh_token_does_not_keep_access_token_in_provider_config() {
    let mut screen = ConfigScreen::new();
    screen.state.sync.provider = SyncProvider::GoogleDrive;
    screen.state.sync.provider_config = Some(ProviderConfig::GoogleDrive(GoogleDriveConfig {
        root_path: ".oak-keyring/".to_string(),
        ..GoogleDriveConfig::default()
    }));

    let (tx, _rx) = mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);

    let result = screen.update(
        Message::CommandCompleted(CommandResult::OAuth2Authorized {
            provider: "google_drive".to_string(),
            access_token: "short_lived_access".to_string(),
            refresh_token: Some("long_lived_refresh".to_string()),
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Command(_)));
    let Some(ProviderConfig::GoogleDrive(cfg)) = &screen.state.sync.provider_config else {
        panic!("expected Google Drive config");
    };
    assert!(cfg.access_token.is_empty());
    assert_eq!(cfg.refresh_token, "long_lived_refresh");
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
