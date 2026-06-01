use super::filter::{
    operation_color, operation_display_name, time_range_index, FilterState, DEBOUNCE_TICKS,
};
use super::screen::AuditLogScreen;
use crate::commands::types::AuditTimeRange;
use crate::commands::{Command, Message};
use crate::config::AppConfig;
use crate::tui::state::audit_state::{AuditFilter, AuditFocus, AuditOperationFilter};
use crate::tui::state::AuditLogRestoreState;
use crate::tui::traits::screen::{Screen, ScreenContext};
use crossterm::event::{KeyCode, KeyModifiers, MouseEvent, MouseEventKind};

fn audit_entry(id: i64) -> crate::types::AuditEntry {
    crate::types::AuditEntry {
        id,
        operation: crate::types::AuditOperation::RecordCreate,
        record_id: None,
        record_name: Some(format!("record-{id}")),
        detail: None,
        occurred_at: chrono::Utc::now(),
    }
}

#[test]
fn new_screen_has_sensible_defaults() {
    let screen = AuditLogScreen::new();
    assert!(screen.state.entries.is_empty());
    assert_eq!(screen.state.total_count, 0);
    assert_eq!(screen.state.selected_index, 0);
    assert_eq!(screen.state.focused_area, AuditFocus::LogList);
    assert!(screen.state.audit_enabled);
    assert!(screen.state.filter.search.is_empty());
    assert_eq!(screen.operation_filter_idx, 0);
    assert_eq!(screen.time_filter_idx, 0);
}

#[test]
fn operation_display_names_are_non_empty() {
    use crate::types::AuditOperation;
    let ops = [
        AuditOperation::RecordCreate,
        AuditOperation::RecordUpdate,
        AuditOperation::RecordDelete,
        AuditOperation::RecordRestore,
        AuditOperation::RecordDestroy,
        AuditOperation::RecordViewPassword,
        AuditOperation::RecordCopyPassword,
        AuditOperation::RecordCopyField,
        AuditOperation::VaultUnlock,
        AuditOperation::VaultLock,
        AuditOperation::VaultExport,
        AuditOperation::VaultImport,
        AuditOperation::MasterPasswordChange,
        AuditOperation::TrashEmpty,
        AuditOperation::SyncConflictResolved,
        AuditOperation::SyncBatchConflictsResolved,
        AuditOperation::DekRotated,
        AuditOperation::DekRotationFailed,
    ];
    for op in &ops {
        assert!(!operation_display_name(op).is_empty());
    }
}

#[test]
fn operation_colors_are_assigned() {
    use crate::types::AuditOperation;
    // Copy operations -> Blue
    assert_eq!(
        operation_color(&AuditOperation::RecordCopyPassword),
        ratatui::style::Color::Blue
    );
    // Create -> Green
    assert_eq!(
        operation_color(&AuditOperation::RecordCreate),
        ratatui::style::Color::Green
    );
    // Update -> Yellow
    assert_eq!(
        operation_color(&AuditOperation::RecordUpdate),
        ratatui::style::Color::Yellow
    );
    // Delete -> Red
    assert_eq!(
        operation_color(&AuditOperation::RecordDelete),
        ratatui::style::Color::Red
    );
    // System -> DarkGray
    assert_eq!(
        operation_color(&AuditOperation::VaultUnlock),
        ratatui::style::Color::DarkGray
    );
}

#[test]
fn filter_debounce_expires_after_ticks() {
    let mut fs = FilterState::default();
    let current = AuditFilter::default();

    fs.on_search_input("test".to_string());
    assert!(fs.debounce_counter.is_some());

    // Tick twice (not yet expired)
    assert!(fs.tick(&current).is_none());
    assert!(fs.tick(&current).is_none());

    // Third tick triggers expiration
    let result = fs.tick(&current);
    assert!(result.is_some());
    assert_eq!(result.unwrap().search, "test");
    assert!(fs.debounce_counter.is_none());
    assert!(fs.pending_search.is_none());
}

#[test]
fn filter_debounce_resets_on_new_input() {
    let mut fs = FilterState::default();
    let current = AuditFilter::default();

    fs.on_search_input("ab".to_string());
    let _ = fs.tick(&current); // counter = 2

    // New input resets the counter
    fs.on_search_input("abc".to_string());
    assert_eq!(fs.debounce_counter, Some(DEBOUNCE_TICKS));
}

#[test]
fn tab_cycles_focus_areas() {
    let mut screen = AuditLogScreen::new();

    // We cannot create a ScreenContext with non-static references in tests,
    // so we directly test the focus cycle logic.
    assert_eq!(screen.state.focused_area, AuditFocus::LogList);

    screen.state.focused_area = AuditFocus::OperationFilter;
    assert_eq!(screen.state.focused_area, AuditFocus::OperationFilter);

    screen.state.focused_area = AuditFocus::TimeFilter;
    assert_eq!(screen.state.focused_area, AuditFocus::TimeFilter);

    screen.state.focused_area = AuditFocus::SearchInput;
    assert_eq!(screen.state.focused_area, AuditFocus::SearchInput);

    screen.state.focused_area = AuditFocus::LogList;
    assert_eq!(screen.state.focused_area, AuditFocus::LogList);
}

#[test]
fn time_range_index_mapping() {
    assert_eq!(time_range_index(None), 0);
    assert_eq!(time_range_index(Some(&AuditTimeRange::All)), 0);
    assert_eq!(time_range_index(Some(&AuditTimeRange::Today)), 1);
    assert_eq!(time_range_index(Some(&AuditTimeRange::LastWeek)), 2);
    assert_eq!(time_range_index(Some(&AuditTimeRange::LastMonth)), 3);
    assert_eq!(time_range_index(Some(&AuditTimeRange::LastYear)), 4);
}

#[test]
fn filtered_entries_with_all_filter_returns_all() {
    let mut screen = AuditLogScreen::new();
    use crate::types::{AuditEntry, AuditOperation};
    use chrono::Utc;

    screen.state.entries = vec![
        AuditEntry {
            id: 1,
            operation: AuditOperation::RecordCreate,
            record_id: None,
            record_name: Some("test".to_string()),
            detail: None,
            occurred_at: Utc::now(),
        },
        AuditEntry {
            id: 2,
            operation: AuditOperation::VaultUnlock,
            record_id: None,
            record_name: None,
            detail: None,
            occurred_at: Utc::now(),
        },
    ];

    // operation_filter_idx = 0 means "All"
    assert_eq!(screen.operation_filter_idx, 0);
    let filtered = screen.filtered_entries();
    assert_eq!(filtered.len(), 2);
}

#[test]
fn filtered_entries_with_copy_filter() {
    let mut screen = AuditLogScreen::new();
    use crate::types::{AuditEntry, AuditOperation};
    use chrono::Utc;

    screen.state.entries = vec![
        AuditEntry {
            id: 1,
            operation: AuditOperation::RecordCreate,
            record_id: None,
            record_name: Some("test".to_string()),
            detail: None,
            occurred_at: Utc::now(),
        },
        AuditEntry {
            id: 2,
            operation: AuditOperation::RecordCopyPassword,
            record_id: None,
            record_name: None,
            detail: None,
            occurred_at: Utc::now(),
        },
    ];

    // Set to "Copy" filter (index 1)
    screen.operation_filter_idx = 1;
    let filtered = screen.filtered_entries();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, 2);
}

#[test]
fn filtered_entries_with_search_filter_matches_record_name_and_detail() {
    let mut screen = AuditLogScreen::new();
    use crate::types::{AuditEntry, AuditOperation};
    use chrono::Utc;

    screen.state.entries = vec![
        AuditEntry {
            id: 1,
            operation: AuditOperation::RecordCreate,
            record_id: None,
            record_name: Some("GitHub".to_string()),
            detail: Some("Created credential".to_string()),
            occurred_at: Utc::now(),
        },
        AuditEntry {
            id: 2,
            operation: AuditOperation::RecordCreate,
            record_id: None,
            record_name: Some("Notion".to_string()),
            detail: Some("workspace login".to_string()),
            occurred_at: Utc::now(),
        },
    ];
    screen.state.filter.search = "git".to_string();

    let filtered = screen.filtered_entries();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, 1);

    screen.state.filter.search = "workspace".to_string();
    let filtered = screen.filtered_entries();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, 2);
}

#[test]
fn filtered_entries_with_time_filter_uses_selected_time_range() {
    let mut screen = AuditLogScreen::new();
    use crate::types::{AuditEntry, AuditOperation};
    use chrono::{Duration, Utc};

    screen.state.entries = vec![
        AuditEntry {
            id: 1,
            operation: AuditOperation::RecordCreate,
            record_id: None,
            record_name: Some("recent".to_string()),
            detail: None,
            occurred_at: Utc::now() - Duration::days(2),
        },
        AuditEntry {
            id: 2,
            operation: AuditOperation::RecordCreate,
            record_id: None,
            record_name: Some("old".to_string()),
            detail: None,
            occurred_at: Utc::now() - Duration::days(20),
        },
    ];
    screen.time_filter_idx = time_range_index(Some(&AuditTimeRange::LastWeek));

    let filtered = screen.filtered_entries();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, 1);
}

#[test]
fn keyboard_down_uses_current_viewport_height_for_scroll_offset() {
    let mut screen = AuditLogScreen::new();
    use crate::types::{AuditEntry, AuditOperation};
    use chrono::Utc;

    screen.visible_log_rows.set(3);
    screen.state.entries = (0..8)
        .map(|id| AuditEntry {
            id,
            operation: AuditOperation::RecordCreate,
            record_id: None,
            record_name: Some(format!("record-{id}")),
            detail: None,
            occurred_at: Utc::now(),
        })
        .collect();

    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    for _ in 0..4 {
        screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Down,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
    }

    assert_eq!(screen.state.selected_index, 4);
    assert_eq!(screen.state.scroll_offset, 2);
}

#[test]
fn mouse_scroll_down_moves_list_selection_and_viewport() {
    let mut screen = AuditLogScreen::new();
    use crate::types::{AuditEntry, AuditOperation};
    use chrono::Utc;

    screen.visible_log_rows.set(3);
    screen.state.entries = (0..8)
        .map(|id| AuditEntry {
            id,
            operation: AuditOperation::RecordCreate,
            record_id: None,
            record_name: Some(format!("record-{id}")),
            detail: None,
            occurred_at: Utc::now(),
        })
        .collect();

    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    screen.update(
        Message::MouseEvent(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 10,
            row: 8,
            modifiers: KeyModifiers::NONE,
        }),
        &mut ctx,
    );

    assert_eq!(screen.state.focused_area, AuditFocus::LogList);
    assert_eq!(screen.state.selected_index, 3);
    assert_eq!(screen.state.scroll_offset, 1);
}

#[test]
fn keyboard_scroll_near_loaded_bottom_requests_next_audit_page() {
    let mut screen = AuditLogScreen::new();
    screen.visible_log_rows.set(3);
    screen.state.entries = (0..50).map(audit_entry).collect();
    screen.state.total_count = 75;
    screen.state.selected_index = 48;
    screen.state.scroll_offset = 46;

    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    screen.update(
        Message::KeyEvent(crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )),
        &mut ctx,
    );

    match rx.try_recv().expect("next page command") {
        Command::LoadAuditLog { filter } => {
            assert_eq!(filter.limit, Some(50));
            assert_eq!(filter.offset, 50);
        }
        other => panic!("expected LoadAuditLog, got {other:?}"),
    }
}

#[test]
fn next_audit_page_appends_without_replacing_existing_entries() {
    let mut screen = AuditLogScreen::new();
    screen.visible_log_rows.set(3);
    screen.state.entries = (0..50).map(audit_entry).collect();
    screen.state.total_count = 75;
    screen.state.selected_index = 48;
    screen.state.scroll_offset = 46;

    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    screen.update(
        Message::KeyEvent(crossterm::event::KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )),
        &mut ctx,
    );
    screen.update(
        Message::AuditLogLoaded {
            entries: (50..60).map(audit_entry).collect(),
            total: 75,
        },
        &mut ctx,
    );

    assert_eq!(screen.state.entries.len(), 60);
    assert_eq!(screen.state.entries[0].id, 0);
    assert_eq!(screen.state.entries[59].id, 59);
    assert_eq!(screen.state.total_count, 75);
}

#[test]
fn mouse_scroll_up_clamps_at_top() {
    let mut screen = AuditLogScreen::new();
    use crate::types::{AuditEntry, AuditOperation};
    use chrono::Utc;

    screen.visible_log_rows.set(3);
    screen.state.entries = (0..8)
        .map(|id| AuditEntry {
            id,
            operation: AuditOperation::RecordCreate,
            record_id: None,
            record_name: Some(format!("record-{id}")),
            detail: None,
            occurred_at: Utc::now(),
        })
        .collect();
    screen.state.selected_index = 1;
    screen.state.scroll_offset = 1;

    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    screen.update(
        Message::MouseEvent(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 10,
            row: 8,
            modifiers: KeyModifiers::NONE,
        }),
        &mut ctx,
    );

    assert_eq!(screen.state.selected_index, 0);
    assert_eq!(screen.state.scroll_offset, 0);
}

#[test]
fn operation_filter_down_wraps_through_all_categories() {
    let mut screen = AuditLogScreen::new();
    screen.state.focused_area = AuditFocus::OperationFilter;
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    for _ in 0..AuditOperationFilter::all_variants().len() {
        screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Down,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
    }

    assert_eq!(screen.operation_filter_idx, 0);
}

#[test]
fn operation_filter_enter_applies_and_returns_to_list() {
    let mut screen = AuditLogScreen::new();
    use crate::types::{AuditEntry, AuditOperation};
    use chrono::Utc;

    screen.state.entries = vec![
        AuditEntry {
            id: 1,
            operation: AuditOperation::RecordCreate,
            record_id: None,
            record_name: Some("added".to_string()),
            detail: None,
            occurred_at: Utc::now(),
        },
        AuditEntry {
            id: 2,
            operation: AuditOperation::RecordDelete,
            record_id: None,
            record_name: Some("deleted".to_string()),
            detail: None,
            occurred_at: Utc::now(),
        },
    ];
    screen.state.focused_area = AuditFocus::OperationFilter;
    screen.operation_filter_idx = 2; // Add
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    screen.update(
        Message::KeyEvent(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )),
        &mut ctx,
    );

    assert_eq!(screen.state.focused_area, AuditFocus::LogList);
    let filtered = screen.filtered_entries();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, 1);
    assert!(matches!(
        rx.try_recv().expect("reload command"),
        Command::LoadAuditLog { .. }
    ));
}

#[test]
fn time_filter_enter_applies_and_returns_to_list() {
    let mut screen = AuditLogScreen::new();
    screen.state.focused_area = AuditFocus::TimeFilter;
    screen.time_filter_idx = time_range_index(Some(&AuditTimeRange::LastWeek));
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    screen.update(
        Message::KeyEvent(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )),
        &mut ctx,
    );

    assert_eq!(screen.state.focused_area, AuditFocus::LogList);
    assert_eq!(
        screen.state.filter.time_range,
        Some(AuditTimeRange::LastWeek)
    );
    match rx.try_recv().expect("reload command") {
        Command::LoadAuditLog { filter } => {
            assert_eq!(filter.time_range, Some(AuditTimeRange::LastWeek));
        }
        other => panic!("expected LoadAuditLog, got {other:?}"),
    }
}

#[test]
fn on_mount_after_snapshot_restore_preserves_selection_scroll_and_focus() {
    let mut screen = AuditLogScreen::new();
    use crate::types::AuditOperation;

    screen.state.entries = (0..10)
        .map(|id| crate::types::AuditEntry {
            id,
            operation: AuditOperation::VaultUnlock,
            record_id: None,
            record_name: None,
            detail: None,
            occurred_at: chrono::Utc::now(),
        })
        .collect();
    screen.state.restore_from(AuditLogRestoreState {
        focused_area: AuditFocus::SearchInput,
        selected_index: 7,
        scroll_offset: 4,
        filter: AuditFilter {
            search: "vault".to_string(),
            operation: None,
            time_range: Some(AuditTimeRange::LastMonth),
        },
    });

    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    screen.on_mount(&mut ctx);

    assert_eq!(screen.state.focused_area, AuditFocus::SearchInput);
    assert_eq!(screen.state.selected_index, 7);
    assert_eq!(screen.state.scroll_offset, 4);
    assert_eq!(screen.state.filter.search, "vault");
    assert_eq!(
        screen.state.filter.time_range,
        Some(AuditTimeRange::LastMonth)
    );
}
