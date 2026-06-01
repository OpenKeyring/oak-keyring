use chrono::TimeZone;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use uuid::Uuid;

use oak_keyring::commands::{Command, Message};
use oak_keyring::config::AppConfig;
use oak_keyring::tui::screens::audit_log::AuditLogScreen;
use oak_keyring::tui::state::audit_state::AuditFocus;
use oak_keyring::tui::traits::screen::Screen;
use oak_keyring::tui::traits::screen::ScreenContext;
use oak_keyring::types::{AuditEntry, AuditOperation};

use crate::support::snapshot_locale;

fn test_context<'a>(
    tx: &'a tokio::sync::mpsc::Sender<Command>,
    config: &'a AppConfig,
) -> ScreenContext<'a> {
    ScreenContext {
        command_tx: tx,
        config,
    }
}

fn key(code: crossterm::event::KeyCode) -> Message {
    Message::KeyEvent(crossterm::event::KeyEvent::new(
        code,
        crossterm::event::KeyModifiers::NONE,
    ))
}

fn render_screen(screen: &AuditLogScreen, width: u16, height: u16) -> TestBackend {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            screen.view(frame, frame.area());
        })
        .unwrap();
    terminal.backend().clone()
}

fn sample_entries() -> Vec<AuditEntry> {
    vec![
        AuditEntry {
            id: 1,
            operation: AuditOperation::VaultUnlock,
            record_id: None,
            record_name: None,
            detail: None,
            occurred_at: chrono::Utc.with_ymd_and_hms(2026, 5, 20, 9, 0, 0).unwrap(),
        },
        AuditEntry {
            id: 2,
            operation: AuditOperation::RecordCreate,
            record_id: Some(Uuid::nil()),
            record_name: Some("Gmail Login".to_string()),
            detail: Some("Created credential for Gmail".to_string()),
            occurred_at: chrono::Utc.with_ymd_and_hms(2026, 5, 20, 9, 5, 0).unwrap(),
        },
        AuditEntry {
            id: 3,
            operation: AuditOperation::RecordViewPassword,
            record_id: Some(Uuid::nil()),
            record_name: Some("Gmail Login".to_string()),
            detail: None,
            occurred_at: chrono::Utc.with_ymd_and_hms(2026, 5, 20, 9, 10, 0).unwrap(),
        },
    ]
}

fn many_entries(count: i64) -> Vec<AuditEntry> {
    (0..count)
        .map(|id| AuditEntry {
            id,
            operation: AuditOperation::RecordCreate,
            record_id: Some(Uuid::nil()),
            record_name: Some(format!("Record {id:02}")),
            detail: None,
            occurred_at: chrono::Utc
                .with_ymd_and_hms(2026, 5, 20, 9, id as u32, 0)
                .unwrap(),
        })
        .collect()
}

#[test]
fn audit_log_empty() {
    let _locale = snapshot_locale();
    let screen = AuditLogScreen::new();
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("audit_log_empty", backend);
}

#[test]
fn audit_log_populated_list_focused() {
    let _locale = snapshot_locale();
    let mut screen = AuditLogScreen::new();
    screen.state.entries = sample_entries();
    screen.state.total_count = 3;
    screen.state.focused_area = AuditFocus::LogList;
    screen.state.selected_index = 1; // Gmail Login creation
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("audit_log_populated_list_focused", backend);
}

#[test]
fn audit_log_long_list_renders_scrolled_view_and_scrollbar() {
    let _locale = snapshot_locale();
    let mut screen = AuditLogScreen::new();
    screen.state.entries = many_entries(20);
    screen.state.total_count = 20;
    screen.state.focused_area = AuditFocus::LogList;
    screen.state.selected_index = 8;
    screen.state.scroll_offset = 6;

    let backend = render_screen(&screen, 80, 10);
    insta::assert_snapshot!(
        "audit_log_long_list_renders_scrolled_view_and_scrollbar",
        backend
    );
}

#[test]
fn audit_log_filter_bar_shows_labels_and_values() {
    let _locale = snapshot_locale();
    let screen = AuditLogScreen::new();
    let backend = render_screen(&screen, 80, 24);
    let rendered = format!("{backend:?}");

    assert!(rendered.contains("Operation"));
    assert!(rendered.contains("Time Range"));
    assert!(rendered.contains("Search"));
    assert!(rendered.contains("All"));
    assert!(rendered.contains("All Time"));
    assert!(rendered.contains("Enter keywords"));
}

#[test]
fn audit_log_operation_filter_focused() {
    let _locale = snapshot_locale();
    let mut screen = AuditLogScreen::new();
    screen.state.entries = sample_entries();
    screen.state.total_count = 3;
    screen.state.focused_area = AuditFocus::OperationFilter;
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);
    let _ = screen.update(key(crossterm::event::KeyCode::Down), &mut ctx);
    let _ = screen.update(key(crossterm::event::KeyCode::Down), &mut ctx);
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("audit_log_operation_filter_focused", backend);
}

#[test]
fn audit_log_time_filter_focused() {
    let _locale = snapshot_locale();
    let mut screen = AuditLogScreen::new();
    screen.state.entries = sample_entries();
    screen.state.total_count = 3;
    screen.state.focused_area = AuditFocus::TimeFilter;
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let config = AppConfig::default();
    let mut ctx = test_context(&tx, &config);
    let _ = screen.update(key(crossterm::event::KeyCode::Down), &mut ctx);
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("audit_log_time_filter_focused", backend);
}

#[test]
fn audit_log_disabled() {
    let _locale = snapshot_locale();
    let mut screen = AuditLogScreen::new();
    screen.state.audit_enabled = false;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("audit_log_disabled", backend);
}
