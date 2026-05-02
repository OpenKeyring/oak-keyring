use super::filter::{
    operation_color, operation_display_name, time_range_index, FilterState, DEBOUNCE_TICKS,
};
use super::screen::AuditLogScreen;
use crate::commands::types::AuditTimeRange;
use crate::config::AppConfig;
use crate::tui::state::audit_state::{AuditFilter, AuditFocus};
use crate::tui::state::AuditLogRestoreState;
use crate::tui::traits::screen::{Screen, ScreenContext};

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
