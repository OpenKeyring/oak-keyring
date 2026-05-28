//! End-to-end pipeline tests: Command -> Result -> State -> Render.
//!
//! Simulates the full message-loop for the main screen, verifying that the
//! three-panel layout correctly responds to keyboard events, command results,
//! and renders without panicking at each step.

#![allow(clippy::field_reassign_with_default)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use oak_keyring::commands::result::CommandResult;
use oak_keyring::commands::types::{
    FieldSelector, HealthIssue, PanelId, RecordFilter, RecordSort, SortDirection, SortField,
};
use oak_keyring::commands::{Command, Message};
use oak_keyring::config::AppConfig;
use oak_keyring::crypto::strength::{PasswordStrength as CryptoStrength, StrengthLevel};
use oak_keyring::tui::screens::main::overlay::ActiveOverlay;
use oak_keyring::tui::state::detail_state::{
    DetailField, DetailFieldKind, DetailViewData, FieldValue, PasswordStrength as DetailStrength,
};
use oak_keyring::tui::state::list_state::ListPanelState;
use oak_keyring::tui::state::main_state::MainScreenState;
use oak_keyring::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use oak_keyring::types::credential::CredentialType;
use oak_keyring::types::record::{DecryptedRecord, TuiRecord};
use oak_keyring::types::SecureStr;
use tokio::sync::mpsc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared Helpers
// ---------------------------------------------------------------------------

/// Create a [`KeyEvent`] from a [`KeyCode`] with no modifiers (press).
fn key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Send a [`KeyEvent`] message to the state and return the [`ScreenResult`].
fn send_key(
    state: &mut MainScreenState,
    ctx: &mut ScreenContext<'_>,
    key_code: KeyCode,
) -> ScreenResult {
    state.update(Message::KeyEvent(key_event(key_code)), ctx)
}

/// Send a [`CommandResult`] message to the state and return the [`ScreenResult`].
fn send_result(
    state: &mut MainScreenState,
    ctx: &mut ScreenContext<'_>,
    cmd_result: CommandResult,
) -> ScreenResult {
    state.update(Message::CommandCompleted(cmd_result), ctx)
}

/// Unwrap a [`ScreenResult::Command`] into its inner [`Command`].
///
/// # Panics
/// Panics if `result` is not `ScreenResult::Command`.
fn extract_command(result: ScreenResult) -> Command {
    match result {
        ScreenResult::Command(cmd) => *cmd,
        other => panic!("Expected ScreenResult::Command, got {other:?}"),
    }
}

/// Create a [`TuiRecord`] with minimal test data.
fn make_test_tui_record(name: &str) -> TuiRecord {
    TuiRecord {
        id: Uuid::new_v4(),
        credential_type: CredentialType::Login,
        name: name.to_string(),
        subtitle: String::new(),
        is_favorite: false,
        is_expired: false,
        expires_at: None,
        has_weak_password: false,
        is_compromised: false,
        duplicate_group_size: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        deleted: false,
        deleted_at: None,
        tags: Vec::new(),
        sync_status: None,
    }
}

/// Build a [`DetailViewData`] with Username, Password, and URL fields.
fn make_detail_view_with_fields(id: Uuid, is_favorite: bool) -> DetailViewData {
    use oak_keyring::tui::state::detail_state::ExpiryStatus;
    DetailViewData {
        id,
        name: "Test Detail".to_string(),
        subtitle: String::new(),
        credential_type: CredentialType::Login,
        is_favorite,
        expires_at: None,
        expiry_status: ExpiryStatus::None,
        tags: Vec::new(),
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        fields: vec![
            DetailField {
                label: "Username".to_string(),
                value: FieldValue::Plain("user".to_string()),
                copyable: true,
                toggleable: false,
                kind: DetailFieldKind::Username,
            },
            DetailField {
                label: "Password".to_string(),
                value: FieldValue::Masked,
                copyable: true,
                toggleable: true,
                kind: DetailFieldKind::Password,
            },
            DetailField {
                label: "URL".to_string(),
                value: FieldValue::Plain("https://example.com".to_string()),
                copyable: true,
                toggleable: false,
                kind: DetailFieldKind::Url,
            },
        ],
        password_strength: None,
        deleted_at: None,
    }
}

/// Render the main screen into a 120x30 [`TestBackend`] and verify no panic.
fn render_and_assert_no_panic(state: &MainScreenState) {
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| state.view(f, Rect::new(0, 0, 120, 30)))
        .unwrap();
}

// ---------------------------------------------------------------------------
// Test 10.1: Full mount -> load list -> load detail -> assert state -> render
// ---------------------------------------------------------------------------

#[test]
fn full_pipeline_mount_to_detail_display() {
    let (tx, mut rx) = mpsc::channel(16);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let mut state = MainScreenState::default();
    state.on_mount(&mut ctx);

    // Step 2: on_mount sends LoadRecordList { filter: All }
    let cmd = rx
        .try_recv()
        .expect("on_mount should send a LoadRecordList command");
    match cmd {
        Command::LoadRecordList { filter, sort } => {
            assert_eq!(filter, RecordFilter::All);
            assert_eq!(
                sort,
                RecordSort {
                    field: SortField::CreatedAt,
                    direction: SortDirection::Desc,
                }
            );
        }
        other => panic!("Expected LoadRecordList command, got {other:?}"),
    }

    // Step 3-4: Construct RecordListLoaded and verify records populated
    let record = make_test_tui_record("GitHub");
    let record_id = record.id;
    let result = send_result(
        &mut state,
        &mut ctx,
        CommandResult::RecordListLoaded {
            records: vec![record],
            total: 1,
            category_counts: oak_keyring::commands::types::RecordCategoryCounts::default(),
        },
    );
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.list.records.len(), 1);

    // Step 5: U4 Empty State — no auto-selection on initial load
    assert_eq!(state.list.selected_index, None);

    // Step 6: Detail shows empty state
    assert!(state.detail.record.is_none());

    // Step 7: Focus List, press j → select first record → LoadRecordDetail
    state.focused_panel = PanelId::List;
    let result = send_key(&mut state, &mut ctx, KeyCode::Char('j'));
    let cmd = extract_command(result);
    match cmd {
        Command::LoadRecordDetail { id } => {
            assert_eq!(id, state.list.records[0].id);
        }
        other => panic!("Expected LoadRecordDetail, got {other:?}"),
    }

    // Step 8: selected_index becomes Some(0)
    assert_eq!(state.list.selected_index, Some(0));

    // Step 9-10: Construct RecordDetailLoaded and verify detail populates
    let crypto_strength = CryptoStrength {
        level: StrengthLevel::Strong,
        char_types: 4,
        bar_fill: 12,
    };
    let decrypted = DecryptedRecord::Login {
        id: record_id,
        name: "GitHub".to_string(),
        username: "octocat".to_string(),
        password: SecureStr::new("ghp_abc123".to_string()),
        url: Some("https://github.com".to_string()),
        notes: None,
        is_favorite: false,
        expires_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        version: 1,
        deleted: false,
        deleted_at: None,
        tags: vec![],
    };
    let result = send_result(
        &mut state,
        &mut ctx,
        CommandResult::RecordDetailLoaded {
            record: decrypted,
            password_strength: Some(crypto_strength),
            health_issue: None,
        },
    );
    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.detail.record.is_some());
    assert_eq!(state.detail.record.as_ref().unwrap().name, "GitHub");

    // Step 12-14: Assert password_strength, password_visible, focused_field
    assert_eq!(
        state.detail.record.as_ref().unwrap().password_strength,
        Some(DetailStrength::Strong)
    );
    assert!(!state.detail.password_visible);
    assert_eq!(state.detail.focused_field, 0);

    // Step 15: Render should not panic
    render_and_assert_no_panic(&state);
}

// ---------------------------------------------------------------------------
// Test 10.2: Password toggle pipeline (p → DecryptField → reveal → mask)
// ---------------------------------------------------------------------------

#[test]
fn password_toggle_pipeline() {
    let (tx, _rx) = mpsc::channel(16);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(record_id, false));

    // First press of 'p' → needs decrypt → returns DecryptField command
    let result = send_key(&mut state, &mut ctx, KeyCode::Char('p'));
    let cmd = extract_command(result);
    match cmd {
        Command::DecryptField { id, field } => {
            assert_eq!(id, record_id);
            assert_eq!(field, FieldSelector::Password);
        }
        other => panic!("Expected DecryptField, got {other:?}"),
    }
    assert!(state.detail.password_visible);

    // FieldDecrypted result → field value changes to Revealed
    let value = SecureStr::new("secret123".to_string());
    let result = send_result(
        &mut state,
        &mut ctx,
        CommandResult::FieldDecrypted {
            id: record_id,
            field: FieldSelector::Password,
            value,
        },
    );
    assert!(matches!(result, ScreenResult::Continue));
    if let Some(ref record) = state.detail.record {
        let password_field = &record.fields[1]; // Password at index 1
        match &password_field.value {
            FieldValue::Revealed(s) => assert_eq!(s, "secret123"),
            other => panic!("Expected Revealed, got {other:?}"),
        }
    }

    // Second press of 'p' → local mask only → Continue
    let result = send_key(&mut state, &mut ctx, KeyCode::Char('p'));
    assert!(matches!(result, ScreenResult::Continue));
    assert!(!state.detail.password_visible);
    if let Some(ref record) = state.detail.record {
        let password_field = &record.fields[1];
        assert!(matches!(password_field.value, FieldValue::Masked));
    }
}

// ---------------------------------------------------------------------------
// Test 10.3: Copy-to-clipboard pipeline (c / u / Enter + CopiedToClipboard)
// ---------------------------------------------------------------------------

#[test]
fn copy_to_clipboard_pipeline() {
    let (tx, _rx) = mpsc::channel(16);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(record_id, false));

    // 'c' → CopyToClipboard { field: Password }
    let result = send_key(&mut state, &mut ctx, KeyCode::Char('c'));
    let cmd = extract_command(result);
    match cmd {
        Command::CopyToClipboard { id, field } => {
            assert_eq!(id, record_id);
            assert_eq!(field, FieldSelector::Password);
        }
        other => panic!("Expected CopyToClipboard(Password), got {other:?}"),
    }

    // 'u' → CopyToClipboard { field: Username }
    let result = send_key(&mut state, &mut ctx, KeyCode::Char('u'));
    let cmd = extract_command(result);
    match cmd {
        Command::CopyToClipboard { id, field } => {
            assert_eq!(id, record_id);
            assert_eq!(field, FieldSelector::Username);
        }
        other => panic!("Expected CopyToClipboard(Username), got {other:?}"),
    }

    // Focus URL field, Enter → CopyToClipboard { field: Url }
    state.detail.focused_field = 2; // URL field is at index 2
    let result = send_key(&mut state, &mut ctx, KeyCode::Enter);
    let cmd = extract_command(result);
    match cmd {
        Command::CopyToClipboard { id, field } => {
            assert_eq!(id, record_id);
            assert_eq!(field, FieldSelector::Url);
        }
        other => panic!("Expected CopyToClipboard(Url), got {other:?}"),
    }

    // CopiedToClipboard result → status_bar.clipboard_countdown
    let result = send_result(
        &mut state,
        &mut ctx,
        CommandResult::CopiedToClipboard {
            field: FieldSelector::Password,
            clear_after_seconds: 30,
        },
    );
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.status_bar.clipboard_countdown, Some(30));
}

// ---------------------------------------------------------------------------
// Test 10.4: Favorite toggle and delete pipeline
// ---------------------------------------------------------------------------

#[test]
fn favorite_and_delete_pipeline() {
    let (tx, mut rx) = mpsc::channel(16);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(record_id, false));
    assert!(!state.detail.record.as_ref().unwrap().is_favorite);

    // 'f' → ToggleFavorite { id, is_favorite: true }
    let result = send_key(&mut state, &mut ctx, KeyCode::Char('f'));
    let cmd = extract_command(result);
    match cmd {
        Command::ToggleFavorite { id, is_favorite } => {
            assert_eq!(id, record_id);
            assert!(is_favorite);
        }
        other => panic!("Expected ToggleFavorite, got {other:?}"),
    }

    // FavoriteToggled result → record.is_favorite == true
    let result = send_result(
        &mut state,
        &mut ctx,
        CommandResult::FavoriteToggled {
            id: record_id,
            is_favorite: true,
        },
    );
    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.detail.record.as_ref().unwrap().is_favorite);

    // 'd' → open delete confirm overlay
    let _result = send_key(&mut state, &mut ctx, KeyCode::Char('d'));
    assert!(state.overlay_manager.is_active());

    // 'y' (universal confirm) → SoftDeleteRecord command
    let result = send_key(&mut state, &mut ctx, KeyCode::Char('y'));
    let cmd = extract_command(result);
    match cmd {
        Command::SoftDeleteRecord { id } => {
            assert_eq!(id, record_id);
        }
        other => panic!("Expected SoftDeleteRecord, got {other:?}"),
    }

    // RecordDeleted result → detail cleared
    let result = send_result(
        &mut state,
        &mut ctx,
        CommandResult::RecordDeleted { id: record_id },
    );
    assert!(matches!(result, ScreenResult::Continue));

    // Drain the LoadRecordList that RecordDeleted sends via channel
    let _cmd = rx
        .try_recv()
        .expect("RecordDeleted should send LoadRecordList");

    // Detail should be none (id matched)
    assert!(state.detail.record.is_none());
}

// ---------------------------------------------------------------------------
// Test 10.5: Sidebar filter change triggers reload and clears detail
// ---------------------------------------------------------------------------

#[test]
fn sidebar_filter_reload_pipeline() {
    let (tx, _rx) = mpsc::channel(16);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let records: Vec<TuiRecord> = (0..3)
        .map(|i| make_test_tui_record(&format!("Record {i}")))
        .collect();
    let first_id = records[0].id;

    let mut state = MainScreenState::default();
    state.list = ListPanelState::with_records(records);
    state.detail.record = Some(make_detail_view_with_fields(first_id, false));
    assert_eq!(state.list.records.len(), 3);
    assert!(state.detail.record.is_some());

    // Focus sidebar, press j → filter changes → LoadRecordList
    state.focused_panel = PanelId::Sidebar;
    let result = send_key(&mut state, &mut ctx, KeyCode::Char('j'));
    let cmd = extract_command(result);
    match cmd {
        Command::LoadRecordList { filter, .. } => {
            assert_eq!(filter, RecordFilter::Favorites);
        }
        other => panic!("Expected LoadRecordList, got {other:?}"),
    }

    // Detail is already cleared by the sidebar handler (before command returned)
    assert!(state.detail.record.is_none());

    // Empty RecordListLoaded → list empty, detail still none
    let result = send_result(
        &mut state,
        &mut ctx,
        CommandResult::RecordListLoaded {
            records: Vec::new(),
            total: 0,
            category_counts: oak_keyring::commands::types::RecordCategoryCounts::default(),
        },
    );
    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.detail.record.is_none());
    assert!(state.list.records.is_empty());
}

// ---------------------------------------------------------------------------
// Test 10.6: Password history overlay pipeline
// ---------------------------------------------------------------------------

#[test]
fn password_history_overlay_pipeline() {
    use oak_keyring::types::PasswordHistoryView;

    let (tx, _rx) = mpsc::channel(16);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(record_id, false));

    // 'H' → LoadPasswordHistory { record_id }
    let result = send_key(&mut state, &mut ctx, KeyCode::Char('H'));
    let cmd = extract_command(result);
    match cmd {
        Command::LoadPasswordHistory { record_id: rid } => {
            assert_eq!(rid, record_id);
        }
        other => panic!("Expected LoadPasswordHistory, got {other:?}"),
    }

    // PasswordHistoryLoaded → overlay active with entries
    let history = vec![PasswordHistoryView {
        id: 1,
        password: SecureStr::new("old-password".to_string()),
        changed_at: chrono::Utc::now(),
    }];
    let result = send_result(
        &mut state,
        &mut ctx,
        CommandResult::PasswordHistoryLoaded { history },
    );
    assert!(matches!(result, ScreenResult::Continue));

    assert!(state.overlay_manager.is_active());
    match state.overlay_manager.get() {
        Some(ActiveOverlay::PasswordHistory(phs)) => {
            assert_eq!(phs.entries.len(), 1);
            assert_eq!(phs.record_name, "Test Detail");
        }
        other => panic!("Expected PasswordHistory overlay, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Test 10.7: List navigation switches detail panel content
// ---------------------------------------------------------------------------

#[test]
fn list_navigation_switches_detail() {
    let (tx, _rx) = mpsc::channel(16);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let records: Vec<TuiRecord> = (0..3)
        .map(|i| make_test_tui_record(&format!("Record {i}")))
        .collect();
    let first_id = records[0].id;
    let second_id = records[1].id;

    let mut state = MainScreenState::default();
    state.list = ListPanelState::with_records(records);
    state.list.selected_index = Some(0);
    state.detail.record = Some(make_detail_view_with_fields(first_id, false));
    assert_eq!(state.detail.record.as_ref().unwrap().id, first_id);

    // Focus list, press j → LoadRecordDetail for records[1]
    state.focused_panel = PanelId::List;
    let result = send_key(&mut state, &mut ctx, KeyCode::Char('j'));
    let cmd = extract_command(result);
    match cmd {
        Command::LoadRecordDetail { id } => {
            assert_eq!(id, second_id);
        }
        other => panic!("Expected LoadRecordDetail for records[1], got {other:?}"),
    }
    assert_eq!(state.list.selected_index, Some(1));

    // RecordDetailLoaded for records[1] → detail shows records[1]
    let decrypted = DecryptedRecord::Login {
        id: second_id,
        name: "Record 1".to_string(),
        username: "user".to_string(),
        password: SecureStr::new("pass".to_string()),
        url: None,
        notes: None,
        is_favorite: false,
        expires_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        version: 1,
        deleted: false,
        deleted_at: None,
        tags: vec![],
    };
    let result = send_result(
        &mut state,
        &mut ctx,
        CommandResult::RecordDetailLoaded {
            record: decrypted,
            password_strength: None,
            health_issue: None,
        },
    );
    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.detail.record.is_some());
    assert_eq!(state.detail.record.as_ref().unwrap().id, second_id);
}

// ---------------------------------------------------------------------------
// Test 10.8: Health issue display in detail panel
// ---------------------------------------------------------------------------

#[test]
fn health_issue_display_pipeline() {
    let (tx, mut rx) = mpsc::channel(16);
    let config = AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let mut state = MainScreenState::default();
    state.on_mount(&mut ctx);

    // Drain the LoadRecordList from on_mount
    let _cmd = rx.try_recv().expect("on_mount should send LoadRecordList");

    // Load a single record into the list
    let record = make_test_tui_record("GitHub");
    let record_id = record.id;
    let _result = send_result(
        &mut state,
        &mut ctx,
        CommandResult::RecordListLoaded {
            records: vec![record],
            total: 1,
            category_counts: oak_keyring::commands::types::RecordCategoryCounts::default(),
        },
    );

    // Focus list, press j → select → LoadRecordDetail
    state.focused_panel = PanelId::List;
    let result = send_key(&mut state, &mut ctx, KeyCode::Char('j'));
    let _cmd = extract_command(result);
    assert_eq!(state.list.selected_index, Some(0));

    // Load detail with HealthIssue::Compromised
    let decrypted = DecryptedRecord::Login {
        id: record_id,
        name: "GitHub".to_string(),
        username: "octocat".to_string(),
        password: SecureStr::new("ghp_abc123".to_string()),
        url: None,
        notes: None,
        is_favorite: false,
        expires_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        version: 1,
        deleted: false,
        deleted_at: None,
        tags: vec![],
    };
    let result = send_result(
        &mut state,
        &mut ctx,
        CommandResult::RecordDetailLoaded {
            record: decrypted,
            password_strength: None,
            health_issue: Some(HealthIssue::Compromised),
        },
    );
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.detail.health_issue, Some(HealthIssue::Compromised));
}
