#![allow(clippy::field_reassign_with_default)]
use super::*;
use crate::commands::types::{
    ConfirmButton, ConfirmDialogState, ConfirmVariant, FieldSelector, Overlay, RecordFilter,
    Screen as ScreenEnum,
};
use crate::commands::{Command, Message};
use crate::config::PasswordGenerationStyle;
use crate::tui::state::list_state::ListPanelState;
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::{SecureStr, Tag};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use tokio::sync::mpsc;
use uuid::Uuid;

#[test]
fn sidebar_default_selects_all_category() {
    let sidebar = SidebarState::default();
    assert!(matches!(
        sidebar.items[sidebar.selected_index],
        SidebarItem::Category(SidebarCategory::All)
    ));
    assert_eq!(sidebar.current_filter(), RecordFilter::All);
}

#[test]
fn sidebar_navigation_skips_separators() {
    let mut sidebar = SidebarState::default();
    sidebar.next_selectable();
    assert!(matches!(
        sidebar.items[sidebar.selected_index],
        SidebarItem::Category(SidebarCategory::Favorites)
    ));

    // Skip ahead past categories to verify separator skip
    sidebar.selected_index = sidebar
        .items
        .iter()
        .position(|i| matches!(i, SidebarItem::Category(SidebarCategory::Trash)))
        .unwrap();
    sidebar.next_selectable();
    assert!(matches!(
        sidebar.items[sidebar.selected_index],
        SidebarItem::TagHeader
    ));
}

#[test]
fn sidebar_prev_navigation_wraps() {
    let mut sidebar = SidebarState::default();
    // Start at index 2 (All), prev should wrap to last selectable item
    sidebar.prev_selectable();
    let last_index = sidebar.selected_index;
    // Last selectable should be Config
    assert!(matches!(sidebar.items[last_index], SidebarItem::Config));
}

#[test]
fn sidebar_select_category() {
    let mut sidebar = SidebarState::default();
    sidebar.select_category(SidebarCategory::Trash);
    assert_eq!(
        sidebar.items[sidebar.selected_index],
        SidebarItem::Category(SidebarCategory::Trash)
    );
    assert_eq!(sidebar.current_filter(), RecordFilter::Trash);
}

#[test]
fn sidebar_tag_filter() {
    let mut sidebar = SidebarState {
        tags_expanded: true,
        tags: vec![Tag {
            id: 1,
            name: "work".to_string(),
        }],
        ..Default::default()
    };
    sidebar.rebuild();

    // Find the tag item and select it
    let tag_idx = sidebar
        .items
        .iter()
        .position(|i| matches!(i, SidebarItem::Tag(_, _)))
        .expect("tag item should exist");
    sidebar.selected_index = tag_idx;
    assert_eq!(
        sidebar.current_filter(),
        RecordFilter::Tag("work".to_string())
    );
}

#[test]
fn sidebar_build_items_structure() {
    let sidebar = SidebarState {
        tags_expanded: true,
        tags: vec![
            Tag {
                id: 1,
                name: "personal".to_string(),
            },
            Tag {
                id: 2,
                name: "work".to_string(),
            },
        ],
        ..Default::default()
    };
    let items = sidebar.build_items();

    assert_eq!(items.len(), 22);

    // Verify structure
    assert!(matches!(items[0], SidebarItem::Spacer));
    assert!(matches!(items[1], SidebarItem::Brand));
    assert!(matches!(items[2], SidebarItem::Separator));
    assert!(matches!(
        items[3],
        SidebarItem::Category(SidebarCategory::All)
    ));
    assert!(matches!(items[12], SidebarItem::Separator));
    assert!(matches!(items[13], SidebarItem::TagHeader));
    assert!(matches!(items[14], SidebarItem::Separator));
    assert!(matches!(items[15], SidebarItem::Tag(ref t, _) if t == "personal"));
    assert!(matches!(items[16], SidebarItem::Separator));
    assert!(matches!(items[17], SidebarItem::Tag(ref t, _) if t == "work"));
    assert!(matches!(items[18], SidebarItem::Separator));
    assert!(matches!(items[19], SidebarItem::Generator));
    assert!(matches!(items[20], SidebarItem::Separator));
    assert!(matches!(items[21], SidebarItem::Config));
}

#[test]
fn sidebar_collapsed_tags_hidden() {
    let sidebar = SidebarState {
        tags_expanded: false, // collapsed
        tags: vec![Tag {
            id: 1,
            name: "work".to_string(),
        }],
        ..Default::default()
    };
    let items = sidebar.build_items();

    // No Tag items should appear when collapsed
    let tag_count = items
        .iter()
        .filter(|i| matches!(i, SidebarItem::Tag(_, _)))
        .count();
    assert_eq!(tag_count, 0);

    // TagHeader still present
    assert!(items.iter().any(|i| matches!(i, SidebarItem::TagHeader)));
}

#[test]
fn main_screen_state_default() {
    let state = MainScreenState::default();
    assert_eq!(state.current_filter, RecordFilter::All);
    assert!(state.pre_lock_snapshot.is_none());
    assert!(matches!(
        state.sidebar.items[state.sidebar.selected_index],
        SidebarItem::Category(SidebarCategory::All)
    ));
    assert_eq!(state.status_bar.record_count, 0);
}

// ── Terminal title tests ──────────────────────────────────────────────

#[test]
fn terminal_title_default_is_empty() {
    let state = TerminalTitleState::default();
    assert!(state.current_title.is_empty());
}

#[test]
fn terminal_title_main_screen() {
    let mut state = TerminalTitleState::default();
    state.set_for_main(None);
    assert_eq!(state.current_title, "OK");
}

#[test]
fn terminal_title_with_record_selected() {
    let mut state = TerminalTitleState::default();
    state.set_for_main(Some("GitHub Account Credentials"));
    assert_eq!(state.current_title, "OK | GitHub Account Credentials");
}

#[test]
fn terminal_title_truncates_long_name() {
    let mut state = TerminalTitleState::default();
    let long_name = "A".repeat(50);
    state.set_for_main(Some(&long_name));
    assert!(state.current_title.len() <= 43);
    assert!(state.current_title.ends_with("..."));
}

#[test]
fn terminal_title_save_and_restore() {
    let mut state = TerminalTitleState::default();
    state.set_for_main(Some("Test"));
    state.save_for_restore();
    assert_eq!(state.pending_restore, Some("OK | Test".to_string()));
    state.set_for_main(None);
    state.restore();
    assert_eq!(state.current_title, "OK | Test");
    assert!(state.pending_restore.is_none());
}

// ── LoadRecordList command and RecordListLoaded handler tests ─────────

#[test]
fn on_mount_sends_load_record_list() {
    use crate::commands::types::RecordFilter;
    use crate::commands::Command;
    use crate::config::AppConfig;
    use crate::tui::traits::screen::{Screen, ScreenContext};
    use tokio::sync::mpsc;

    let config = AppConfig::default();
    let (tx, mut rx) = mpsc::channel(16);

    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let mut state = MainScreenState::default();
    state.on_mount(&mut ctx);

    let cmd = rx
        .try_recv()
        .expect("on_mount should send a LoadRecordList command");
    match cmd {
        Command::LoadRecordList {
            filter,
            sort,
            limit,
            offset,
        } => {
            assert_eq!(filter, RecordFilter::All);
            assert_eq!(sort.field, crate::commands::types::SortField::CreatedAt);
            assert_eq!(sort.direction, crate::commands::types::SortDirection::Desc);
            assert_eq!(limit, 500);
            assert_eq!(offset, 0);
        }
        _ => panic!("Expected LoadRecordList command, got a different command"),
    }
}

#[test]
fn on_mount_sends_load_record_list_with_current_filter() {
    use crate::commands::types::RecordFilter;
    use crate::commands::Command;
    use crate::config::AppConfig;
    use crate::tui::traits::screen::{Screen, ScreenContext};
    use tokio::sync::mpsc;

    let config = AppConfig::default();
    let (tx, mut rx) = mpsc::channel(16);

    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let mut state = MainScreenState {
        current_filter: RecordFilter::Favorites,
        ..Default::default()
    };
    state.on_mount(&mut ctx);

    let cmd = rx
        .try_recv()
        .expect("on_mount should send a LoadRecordList command");
    match cmd {
        Command::LoadRecordList {
            filter,
            limit,
            offset,
            ..
        } => {
            assert_eq!(filter, RecordFilter::Favorites);
            assert_eq!(limit, 500);
            assert_eq!(offset, 0);
        }
        _ => panic!("Expected LoadRecordList command"),
    }
}

#[test]
fn record_list_scroll_near_loaded_bottom_requests_next_page() {
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::List;
    state.terminal_area = Rect::new(0, 0, 120, 30);
    state.list.records = (0..500)
        .map(|idx| make_test_record_with_name(Uuid::new_v4(), &format!("record-{idx:03}")))
        .collect();
    state.list.total_count = 600;
    state.list.selected_index = Some(497);
    state.list.scroll_offset = 490;
    state.list.set_visible_height(8);

    let layout = crate::tui::screens::main::layout::calculate_layout(state.terminal_area, 120);
    let list_rect = Rect::new(
        layout.list.x,
        layout.list.y + 2,
        layout.list.width,
        layout.list.height.saturating_sub(2),
    );
    let event = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::ScrollDown,
        column: list_rect.x + 1,
        row: list_rect.y + 3,
        modifiers: KeyModifiers::NONE,
    };

    let config = crate::config::AppConfig::default();
    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(Message::MouseEvent(event), &mut ctx);

    match result {
        ScreenResult::Command(cmd) => match *cmd {
            Command::LoadRecordList {
                filter,
                limit,
                offset,
                ..
            } => {
                assert_eq!(filter, RecordFilter::All);
                assert_eq!(limit, 500);
                assert_eq!(offset, 500);
            }
            other => panic!("expected LoadRecordList, got {other:?}"),
        },
        other => panic!("expected command result, got {other:?}"),
    }
}

#[test]
fn record_list_loaded_appends_next_page_without_replacing_existing_records() {
    use crate::commands::result::CommandResult;

    let mut state = MainScreenState::default();
    state.list.records = (0..500)
        .map(|idx| make_test_record_with_name(Uuid::new_v4(), &format!("record-{idx:03}")))
        .collect();
    state.list.total_count = 600;
    state.list.pending_load_offset = Some(500);
    let next_page: Vec<_> = (500..525)
        .map(|idx| make_test_record_with_name(Uuid::new_v4(), &format!("record-{idx:03}")))
        .collect();

    let config = crate::config::AppConfig::default();
    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: next_page,
            total: 600,
            category_counts: Default::default(),
        }),
        &mut ctx,
    );

    assert_eq!(state.list.records.len(), 525);
    assert_eq!(state.list.records[0].name, "record-000");
    assert_eq!(state.list.records[524].name, "record-524");
    assert_eq!(state.list.total_count, 600);
}

#[test]
fn record_list_loaded_populates_records_and_total() {
    use crate::commands::result::CommandResult;
    use crate::commands::Message;
    use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;
    use tokio::sync::mpsc;

    let records = vec![
        TuiRecord {
            id: uuid::Uuid::new_v4(),
            credential_type: CredentialType::Login,
            name: "Test 1".to_string(),
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
        },
        TuiRecord {
            id: uuid::Uuid::new_v4(),
            credential_type: CredentialType::Login,
            name: "Test 2".to_string(),
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
        },
    ];

    let mut state = MainScreenState::default();
    assert!(state.list.records.is_empty());
    assert_eq!(state.list.total_count, 0);
    assert_eq!(state.status_bar.record_count, 0);
    assert_eq!(state.list.selected_index, None);

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records,
            total: 10,
            category_counts: crate::commands::types::RecordCategoryCounts::default(),
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.list.records.len(), 2);
    assert_eq!(state.list.total_count, 10);
    // selected_index stays None — no auto-select (U4 Empty State)
    assert_eq!(state.list.selected_index, None);
}

#[test]
fn record_list_loaded_updates_status_bar_count() {
    use crate::commands::result::CommandResult;
    use crate::commands::Message;
    use crate::tui::traits::screen::{Screen, ScreenContext};
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;
    use tokio::sync::mpsc;

    let records = vec![TuiRecord {
        id: uuid::Uuid::new_v4(),
        credential_type: CredentialType::Login,
        name: "Test".to_string(),
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
    }];

    let mut state = MainScreenState::default();
    assert_eq!(state.status_bar.record_count, 0);

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let _result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records,
            total: 5,
            category_counts: crate::commands::types::RecordCategoryCounts::default(),
        }),
        &mut ctx,
    );

    assert_eq!(state.status_bar.record_count, 5);
}

#[test]
fn record_list_loaded_updates_sidebar_category_counts() {
    use crate::commands::result::CommandResult;
    use crate::commands::types::RecordCategoryCounts;
    use crate::commands::Message;
    use crate::tui::traits::screen::{Screen, ScreenContext};
    use tokio::sync::mpsc;

    let mut state = MainScreenState::default();

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let _ = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: Vec::new(),
            total: 3,
            category_counts: RecordCategoryCounts {
                all: 3,
                favorites: 1,
                expired: 1,
                health_issues: 2,
                trash: 1,
            },
        }),
        &mut ctx,
    );

    assert_eq!(state.sidebar.category_counts.all, 3);
    assert_eq!(state.sidebar.category_counts.favorites, 1);
    assert_eq!(state.sidebar.category_counts.expired, 1);
    assert_eq!(state.sidebar.category_counts.health_issues, 2);
    assert_eq!(state.sidebar.category_counts.trash, 1);
}

#[test]
fn record_list_loaded_handles_empty_list() {
    use crate::commands::result::CommandResult;
    use crate::commands::Message;
    use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
    use tokio::sync::mpsc;

    let mut state = MainScreenState::default();

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: Vec::new(),
            total: 0,
            category_counts: crate::commands::types::RecordCategoryCounts::default(),
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.list.records.is_empty());
    assert_eq!(state.list.total_count, 0);
    assert_eq!(state.status_bar.record_count, 0);
    assert_eq!(state.list.selected_index, None);
}

// ── Sidebar navigation tests ──────────────────────────────────────────

#[test]
fn sidebar_move_down() {
    let mut state = SidebarState::default();
    let prev = state.selected_index;
    state.move_down();
    assert_ne!(state.selected_index, prev);
    assert!(state.items[state.selected_index].is_selectable());
}

#[test]
fn sidebar_move_up() {
    let mut state = SidebarState::default();
    state.selected_index = state
        .items
        .iter()
        .position(|i| matches!(i, SidebarItem::Category(SidebarCategory::Favorites)))
        .unwrap();
    state.move_up();
    assert!(matches!(
        state.items[state.selected_index],
        SidebarItem::Category(SidebarCategory::All)
    ));
}

#[test]
fn sidebar_toggle_tags() {
    let mut state = SidebarState {
        tags_expanded: true,
        tags: vec![Tag {
            id: 1,
            name: "work".into(),
        }],
        ..Default::default()
    };
    state.rebuild();
    assert!(state
        .items
        .iter()
        .any(|i| matches!(i, SidebarItem::Tag(_, _))));
    state.toggle_tags();
    assert!(!state.tags_expanded);
    assert!(state
        .items
        .iter()
        .all(|i| !matches!(i, SidebarItem::Tag(_, _))));
    state.toggle_tags();
    assert!(state.tags_expanded);
}

#[test]
fn tag_header_is_selectable() {
    assert!(SidebarItem::TagHeader.is_selectable());
}

#[test]
fn tag_header_keyboard_navigable() {
    let mut sidebar = SidebarState::default();
    sidebar.selected_index = sidebar
        .items
        .iter()
        .position(|i| matches!(i, SidebarItem::Category(SidebarCategory::Trash)))
        .unwrap();
    sidebar.move_down();
    assert!(matches!(
        sidebar.items[sidebar.selected_index],
        SidebarItem::TagHeader
    ));
    // Move down again — should skip Separator and land on Generator.
    sidebar.move_down();
    assert!(matches!(
        sidebar.items[sidebar.selected_index],
        SidebarItem::Generator
    ));
}

#[test]
fn tag_header_toggle_expands_and_collapses() {
    let mut sidebar = SidebarState::default();
    // Navigate to TagHeader
    let header_idx = sidebar
        .items
        .iter()
        .position(|i| matches!(i, SidebarItem::TagHeader))
        .unwrap();
    sidebar.selected_index = header_idx;
    assert!(sidebar.tags_expanded);

    // Toggle: expanded -> collapsed, focus returns to TagHeader
    sidebar.toggle_tags();
    assert!(!sidebar.tags_expanded);
    assert!(matches!(
        sidebar.items[sidebar.selected_index],
        SidebarItem::TagHeader
    ));

    // Toggle: collapsed -> expanded
    sidebar.toggle_tags();
    assert!(sidebar.tags_expanded);
}

#[test]
fn tag_header_collapse_returns_focus_to_header() {
    let mut sidebar = SidebarState {
        tags_expanded: true,
        tags: vec![Tag {
            id: 1,
            name: "work".into(),
        }],
        ..Default::default()
    };
    sidebar.rebuild();
    // Select a tag item
    let tag_idx = sidebar
        .items
        .iter()
        .position(|i| matches!(i, SidebarItem::Tag(_, _)))
        .unwrap();
    sidebar.selected_index = tag_idx;

    // Select TagHeader and collapse
    let header_idx = sidebar
        .items
        .iter()
        .position(|i| matches!(i, SidebarItem::TagHeader))
        .unwrap();
    sidebar.selected_index = header_idx;
    sidebar.toggle_tags();

    // After collapse, focus should be on TagHeader
    assert!(!sidebar.tags_expanded);
    assert!(matches!(
        sidebar.items[sidebar.selected_index],
        SidebarItem::TagHeader
    ));
}

// ── Tag delete and visual/search mutual exclusion tests ──────────────

#[test]
fn tag_delete_auto_switches_to_all_when_viewing_deleted_tag() {
    let mut state = MainScreenState {
        current_filter: RecordFilter::Tag("work".to_string()),
        ..Default::default()
    };
    state.sidebar.select_category(SidebarCategory::All);

    // Simulate tag deletion: switch filter to All
    state.current_filter = RecordFilter::All;
    state.sidebar.select_category(SidebarCategory::All);

    assert_eq!(state.current_filter, RecordFilter::All);
    assert_eq!(
        state.sidebar.items[state.sidebar.selected_index],
        SidebarItem::Category(SidebarCategory::All)
    );
}

#[test]
fn visual_mode_and_search_are_mutually_exclusive() {
    let mut state = MainScreenState::default();
    state.list.enter_visual();
    assert!(state.list.is_visual());
    assert!(!state.list.is_searching());

    // Entering search should exit visual
    state.list.exit_visual();
    state.list.enter_search();
    assert!(state.list.is_searching());
    assert!(!state.list.is_visual());

    // Entering visual should exit search
    state.list.commit_search();
    state.list.enter_visual();
    assert!(state.list.is_visual());
    assert!(!state.list.is_searching());
}

#[test]
fn trash_retention_days_default_is_30() {
    let state = MainScreenState::default();
    assert_eq!(state.trash_retention_days, 30);
    assert_eq!(state.detail.trash_retention_days, 30);
}

#[test]
fn on_mount_reads_trash_retention_from_config() {
    use crate::config::AppConfig;
    use crate::tui::traits::screen::{Screen, ScreenContext};
    use tokio::sync::mpsc;

    let mut config = AppConfig::default();
    config.general.trash_retention_days = 60;
    let (tx, _rx) = mpsc::channel(1);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let mut state = MainScreenState::default();
    assert_eq!(state.trash_retention_days, 30);
    assert_eq!(state.detail.trash_retention_days, 30);

    state.on_mount(&mut ctx);

    assert_eq!(state.trash_retention_days, 60);
    assert_eq!(state.detail.trash_retention_days, 60);
}

#[test]
fn on_mount_reads_zero_retention_from_config() {
    use crate::config::AppConfig;
    use crate::tui::traits::screen::{Screen, ScreenContext};
    use tokio::sync::mpsc;

    let mut config = AppConfig::default();
    config.general.trash_retention_days = 0;
    let (tx, _rx) = mpsc::channel(1);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let mut state = MainScreenState::default();
    state.on_mount(&mut ctx);

    assert_eq!(state.trash_retention_days, 0);
    assert_eq!(state.detail.trash_retention_days, 0);
}

#[test]
fn main_restore_state_restores_navigation_context() {
    let mut state = MainScreenState::default();
    state.sidebar.selected_index = 2;
    state.sidebar.tags_expanded = true;
    state.sidebar.tag_scroll_offset = 3;
    state.list.selected_index = Some(4);
    state.list.scroll_offset = 2;
    state.detail.focused_field = 5;

    let restore = state.to_restore_state(crate::commands::types::PanelId::Detail);

    let mut restored = MainScreenState::default();
    restored.restore_from(restore.clone());

    assert_eq!(
        restore.focused_panel,
        crate::commands::types::PanelId::Detail
    );
    assert_eq!(restored.sidebar.selected_index, 2);
    assert!(restored.sidebar.tags_expanded);
    assert_eq!(restored.sidebar.tag_scroll_offset, 3);
    assert_eq!(restored.list.selected_index, Some(4));
    assert_eq!(restored.list.scroll_offset, 2);
    assert_eq!(restored.detail.focused_field, 5);
}

// ── Overlay integration tests ─────────────────────────────────────────────

use crate::commands::types::PanelId;
use crate::tui::state::animation::EffectKind;

fn make_ctx() -> ScreenContext<'static> {
    // Leak is acceptable in tests — the channel lives for the process lifetime.
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    ScreenContext {
        command_tx: Box::leak(Box::new(tx)),
        config: Box::leak(Box::new(crate::config::AppConfig::default())),
    }
}

fn key_event(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
    use crossterm::event::{KeyEvent, KeyEventKind, KeyModifiers};
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }
}

fn key_event_with_modifiers(
    code: crossterm::event::KeyCode,
    modifiers: crossterm::event::KeyModifiers,
) -> crossterm::event::KeyEvent {
    use crossterm::event::{KeyEvent, KeyEventKind};
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }
}

fn mouse_move(column: u16, row: u16) -> crossterm::event::MouseEvent {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    MouseEvent {
        kind: MouseEventKind::Moved,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn mouse_click(column: u16, row: u16) -> crossterm::event::MouseEvent {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn default_state_has_no_active_overlay() {
    let state = MainScreenState::default();
    assert!(!state.overlay_manager.is_active());
    assert!(state.pending_animation.is_none());
}

#[test]
fn sidebar_tags_are_expanded_by_default() {
    let state = MainScreenState::default();
    assert!(state.sidebar.tags_expanded);
}

#[test]
fn ctrl_k_enters_search_from_sidebar() {
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Sidebar;
    let mut ctx = make_ctx();

    let result = state.update(
        Message::KeyEvent(key_event_with_modifiers(
            KeyCode::Char('k'),
            KeyModifiers::CONTROL,
        )),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.focused_panel, PanelId::List);
    assert!(state.list.is_searching());
}

#[test]
fn left_right_arrows_switch_main_panels() {
    let mut state = MainScreenState::default();
    let mut ctx = make_ctx();

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Right)), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.focused_panel, PanelId::List);

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Right)), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.focused_panel, PanelId::Detail);

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Left)), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.focused_panel, PanelId::List);
}

#[test]
fn right_from_sidebar_selects_first_list_item_when_list_has_no_selection() {
    let mut state = MainScreenState::default();
    let first = make_test_record(None);
    let first_id = first.id;
    let second = make_test_record(None);
    state.list.records = vec![first, second];
    state.list.selected_index = None;
    state.focused_panel = PanelId::Sidebar;
    let mut ctx = make_ctx();

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Right)), &mut ctx);

    assert_eq!(state.focused_panel, PanelId::List);
    assert_eq!(state.list.selected_index, Some(0));
    match result {
        ScreenResult::Command(cmd) => match *cmd {
            Command::LoadRecordDetail { id } => assert_eq!(id, first_id),
            other => panic!("expected LoadRecordDetail, got {other:?}"),
        },
        other => panic!("expected LoadRecordDetail command, got {other:?}"),
    }
}

#[test]
fn number_shortcuts_select_sidebar_categories() {
    let mut state = MainScreenState::default();
    let mut ctx = make_ctx();

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Char('4'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Command(_)));
    assert_eq!(state.current_filter, RecordFilter::HealthIssues);
    assert_eq!(state.focused_panel, PanelId::Sidebar);
}

#[test]
fn ctrl_g_opens_generator_and_ctrl_p_opens_config() {
    let mut state = MainScreenState::default();
    let mut ctx = make_ctx();

    let result = state.update(
        Message::KeyEvent(key_event_with_modifiers(
            KeyCode::Char('g'),
            KeyModifiers::CONTROL,
        )),
        &mut ctx,
    );
    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.overlay_manager.is_active());

    let mut state = MainScreenState::default();
    let result = state.update(
        Message::KeyEvent(key_event_with_modifiers(
            KeyCode::Char('p'),
            KeyModifiers::CONTROL,
        )),
        &mut ctx,
    );
    assert!(matches!(
        result,
        ScreenResult::NavigateTo(ScreenEnum::Config)
    ));
}

#[test]
fn number_six_opens_generator_and_seven_opens_config() {
    let mut state = MainScreenState::default();
    let mut ctx = make_ctx();

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Char('6'))), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.overlay_manager.is_active());

    let mut state = MainScreenState::default();
    let result = state.update(Message::KeyEvent(key_event(KeyCode::Char('7'))), &mut ctx);
    assert!(matches!(
        result,
        ScreenResult::NavigateTo(ScreenEnum::Config)
    ));
}

#[test]
fn number_zero_focuses_sidebar_tag_header() {
    let mut state = MainScreenState::default();
    state.sidebar.tags = vec![Tag {
        id: 1,
        name: "work".to_string(),
    }];
    state.sidebar.rebuild();
    let mut ctx = make_ctx();

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Char('0'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.focused_panel, PanelId::Sidebar);
    assert!(matches!(
        state.sidebar.items[state.sidebar.selected_index],
        SidebarItem::TagHeader
    ));
}

#[test]
fn mouse_hover_list_row_only_focuses_list() {
    let mut state = MainScreenState::default();
    let mut first = make_test_record(None);
    first.name = "First".to_string();
    let mut second = make_test_record(None);
    second.name = "Second".to_string();
    state.list = ListPanelState::with_records(vec![first, second]);
    state.focused_panel = PanelId::Sidebar;
    let mut ctx = make_ctx();

    let result = state.update(Message::MouseEvent(mouse_move(42, 7)), &mut ctx);

    assert_eq!(state.focused_panel, PanelId::List);
    assert_eq!(state.list.selected_index, Some(0));
    assert!(matches!(result, ScreenResult::Continue));
}

#[test]
fn mouse_click_list_row_selects_record_and_loads_detail() {
    let mut state = MainScreenState::default();
    let mut first = make_test_record(None);
    first.name = "First".to_string();
    let mut second = make_test_record(None);
    second.name = "Second".to_string();
    let second_id = second.id;
    state.list = ListPanelState::with_records(vec![first, second]);
    state.focused_panel = PanelId::Sidebar;
    let mut ctx = make_ctx();

    let result = state.update(Message::MouseEvent(mouse_click(42, 7)), &mut ctx);

    assert_eq!(state.focused_panel, PanelId::List);
    assert_eq!(state.list.selected_index, Some(1));
    match result {
        ScreenResult::Command(cmd) => match *cmd {
            Command::LoadRecordDetail { id } => assert_eq!(id, second_id),
            other => panic!("expected LoadRecordDetail, got {other:?}"),
        },
        other => panic!("expected command result, got {other:?}"),
    }
}

#[test]
fn mouse_click_list_row_uses_rendered_scroll_offset() {
    let mut state = MainScreenState::default();
    state.terminal_area = Rect::new(0, 0, 120, 30);
    state.list.records = (0..10)
        .map(|idx| make_test_record_with_name(Uuid::new_v4(), &format!("record-{idx}")))
        .collect();
    state.list.total_count = state.list.records.len();
    state.list.selected_index = None;
    state.list.scroll_offset = 8;
    let expected_id = state.list.records[2].id;
    let mut ctx = make_ctx();

    let layout = crate::tui::screens::main::layout::calculate_layout(state.terminal_area, 120);
    let list_rect = Rect::new(
        layout.list.x,
        layout.list.y + 1,
        layout.list.width,
        layout.list.height.saturating_sub(1),
    );
    let result = state.update(
        Message::MouseEvent(mouse_click(list_rect.x + 2, list_rect.y + 2)),
        &mut ctx,
    );

    assert_eq!(state.focused_panel, PanelId::List);
    assert_eq!(state.list.selected_index, Some(2));
    match result {
        ScreenResult::Command(cmd) => match *cmd {
            Command::LoadRecordDetail { id } => assert_eq!(id, expected_id),
            other => panic!("expected LoadRecordDetail, got {other:?}"),
        },
        other => panic!("expected command result, got {other:?}"),
    }
}

#[test]
fn mouse_click_list_sort_bar_changes_sort() {
    let mut state = MainScreenState::default();
    let original = state.current_sort.field;
    let mut ctx = make_ctx();

    // Default terminal_area is Rect(0,0,100,24). With sidebar_width=40,
    // list_rect = top_padded(Rect(42,0,17,22), 1) = Rect(42,1,17,21).
    // Click (45,1): row_in_list=0 (sort bar), col_in_list=3 < 8 → cycle_sort_field.
    let result = state.update(Message::MouseEvent(mouse_click(45, 1)), &mut ctx);

    assert!(matches!(result, ScreenResult::Command(_)));
    assert_ne!(state.current_sort.field, original);
    assert_eq!(state.focused_panel, PanelId::List);
}

#[test]
fn p_key_opens_generator_overlay() {
    let mut state = MainScreenState::default();
    let mut ctx = make_ctx();
    let result = state.update(Message::KeyEvent(key_event(KeyCode::Char('p'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.overlay_manager.is_active());
    assert_eq!(state.pending_animation, Some(EffectKind::ModalAppear));
}

#[test]
fn p_key_opens_generator_overlay_with_configured_defaults() {
    let mut state = MainScreenState::default();
    state.password_defaults.style = PasswordGenerationStyle::Pin;
    state.password_defaults.pin_length = 10;
    let mut ctx = make_ctx();

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Char('p'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    match state.overlay_manager.get() {
        Some(crate::tui::screens::main::overlay::ActiveOverlay::PasswordGenerator(gen)) => {
            assert_eq!(
                gen.style,
                crate::tui::state::generator_state::GenerationStyle::Pin
            );
            assert_eq!(gen.pin_config.length, 10);
        }
        other => panic!("expected password generator overlay, got {other:?}"),
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn sidebar_generator_opens_overlay() {
    let mut state = MainScreenState::default();
    // Find Generator item in sidebar and select it
    let gen_idx = state
        .sidebar
        .items
        .iter()
        .position(|i| matches!(i, SidebarItem::Generator))
        .expect("Generator should be in sidebar");
    state.sidebar.selected_index = gen_idx;
    state.focused_panel = PanelId::Sidebar;

    let mut ctx = make_ctx();
    let result = state.update(Message::KeyEvent(key_event(KeyCode::Enter)), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.overlay_manager.is_active());
    assert_eq!(state.pending_animation, Some(EffectKind::ModalAppear));
}

#[test]
fn esc_closes_overlay_with_dismiss_animation() {
    let mut state = MainScreenState::default();
    // Open overlay first
    state
        .overlay_manager
        .open(crate::commands::types::Overlay::PasswordGenerator);
    assert!(state.overlay_manager.is_active());

    let mut ctx = make_ctx();
    let result = state.update(Message::KeyEvent(key_event(KeyCode::Esc)), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert!(!state.overlay_manager.is_active());
    assert_eq!(state.pending_animation, Some(EffectKind::ModalDismiss));
}

#[test]
fn overlay_consumes_tab_key() {
    let mut state = MainScreenState::default();
    state
        .overlay_manager
        .open(crate::commands::types::Overlay::PasswordGenerator);

    let mut ctx = make_ctx();
    let result = state.update(Message::KeyEvent(key_event(KeyCode::Tab)), &mut ctx);

    // Tab should be consumed by overlay, not propagate to panel focus cycling
    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.overlay_manager.is_active());
}

#[test]
fn copy_generated_password_maps_to_command() {
    let mut state = MainScreenState::default();
    state.overlay_manager.open(Overlay::PasswordGenerator);
    assert!(state.overlay_manager.is_active());

    // Set a preview password in the generator state
    if let Some(crate::tui::screens::main::overlay::ActiveOverlay::PasswordGenerator(gen_state)) =
        state.overlay_manager.get_mut()
    {
        gen_state.preview =
            crate::types::sensitive::SensitiveInput::from("test-password-123".to_string());
        gen_state.focus = crate::tui::state::generator_state::GeneratorFocus::ActionButton;
    }

    // Enter key triggers copy in generator overlay
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };
    let result = state.update(Message::KeyEvent(key), &mut ctx);

    // Overlay should be closed
    assert!(!state.overlay_manager.is_active());

    // Should return a CopyRawToClipboard command
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::CopyRawToClipboard { .. } => {}
            other => panic!("Expected CopyRawToClipboard, got {:?}", other),
        }
    } else {
        panic!("Expected Command result, got {:?}", result);
    }
}

#[test]
fn q_key_opens_quit_confirm_on_main_screen() {
    let mut state = MainScreenState::default();
    let mut ctx = make_ctx();

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Char('q'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    match state.overlay_manager.get() {
        Some(crate::tui::screens::main::overlay::ActiveOverlay::ConfirmDialog {
            variant,
            focused_button,
        }) => {
            assert!(matches!(variant, ConfirmVariant::QuitApp));
            assert_eq!(*focused_button, ConfirmButton::Cancel);
        }
        other => panic!("expected quit confirm overlay, got {other:?}"),
    }
}

#[test]
fn quit_confirm_requires_explicit_confirm_before_exit() {
    let mut state = MainScreenState::default();
    let mut ctx = make_ctx();

    let _ = state.update(Message::KeyEvent(key_event(KeyCode::Char('q'))), &mut ctx);

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Enter)), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert!(!state.overlay_manager.is_active());

    let _ = state.update(Message::KeyEvent(key_event(KeyCode::Char('q'))), &mut ctx);
    let _ = state.update(Message::KeyEvent(key_event(KeyCode::Tab)), &mut ctx);
    let result = state.update(Message::KeyEvent(key_event(KeyCode::Enter)), &mut ctx);
    assert!(matches!(result, ScreenResult::ExitApp));
}

#[test]
fn confirm_soft_delete_maps_to_command() {
    let mut state = MainScreenState::default();
    let record_id = Uuid::new_v4();
    let dialog = ConfirmDialogState {
        variant: ConfirmVariant::SoftDelete {
            record_id,
            record_name: "Test Record".to_string(),
            auto_delete_days: None,
        },
        focused_button: ConfirmButton::Confirm,
    };
    assert!(state.overlay_manager.open(Overlay::ConfirmDialog(dialog)));
    assert!(state.overlay_manager.is_active());

    // Enter confirms (focused on Confirm button)
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };
    let result = state.update(Message::KeyEvent(key), &mut ctx);

    assert!(!state.overlay_manager.is_active());
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::SoftDeleteRecord { id } if *id == record_id => {}
            other => panic!(
                "Expected SoftDeleteRecord with id {:?}, got {:?}",
                record_id, other
            ),
        }
    } else {
        panic!("Expected Command result, got {:?}", result);
    }
}

#[test]
fn confirm_empty_trash_maps_to_command() {
    let mut state = MainScreenState::default();
    let dialog = ConfirmDialogState {
        variant: ConfirmVariant::EmptyTrash { count: 5 },
        focused_button: ConfirmButton::Cancel, // EmptyTrash defaults to Cancel for safety
    };
    assert!(state.overlay_manager.open(Overlay::ConfirmDialog(dialog)));
    assert!(state.overlay_manager.is_active());

    // 'y' always confirms regardless of which button has focus
    let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };
    let result = state.update(Message::KeyEvent(key), &mut ctx);

    assert!(!state.overlay_manager.is_active());
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::EmptyTrash => {}
            other => panic!("Expected EmptyTrash, got {:?}", other),
        }
    } else {
        panic!("Expected Command result, got {:?}", result);
    }
}

// ── Sidebar filter change -> LoadRecordList tests ──────────────────────────

#[test]
fn sidebar_j_triggers_filter_change_and_reload() {
    let mut state = MainScreenState {
        focused_panel: PanelId::Sidebar,
        ..Default::default()
    };
    // Default starts at All (index 2), j moves to Favorites (index 3)

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Char('j'))), &mut ctx);

    // Should return a LoadRecordList command
    assert!(matches!(result, ScreenResult::Command(_)));
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::LoadRecordList { filter, .. } => {
                assert_eq!(*filter, RecordFilter::Favorites);
            }
            other => panic!("Expected LoadRecordList command, got {:?}", other),
        }
    }

    // Filter should be updated
    assert_eq!(state.current_filter, RecordFilter::Favorites);
    // list_auto_select should be set for the next RecordListLoaded
    assert!(state.list_auto_select);
}

#[test]
fn sidebar_k_triggers_filter_change_and_reload() {
    let mut state = MainScreenState {
        focused_panel: PanelId::Sidebar,
        current_filter: RecordFilter::Favorites,
        ..Default::default()
    };
    // Start at Favorites, k moves up to All
    state.sidebar.select_category(SidebarCategory::Favorites);
    assert_eq!(state.current_filter, RecordFilter::Favorites);

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Char('k'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Command(_)));
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::LoadRecordList { filter, .. } => {
                assert_eq!(*filter, RecordFilter::All);
            }
            other => panic!("Expected LoadRecordList command, got {:?}", other),
        }
    }
    assert_eq!(state.current_filter, RecordFilter::All);
    assert!(state.list_auto_select);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn sidebar_j_down_clears_detail() {
    use crate::tui::state::detail_state::ExpiryStatus;

    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Sidebar;

    // Set up a detail record so we can verify it gets cleared
    state.detail.record = Some(crate::tui::state::detail_state::DetailViewData {
        id: Uuid::new_v4(),
        name: "Test".to_string(),
        subtitle: String::new(),
        credential_type: crate::types::credential::CredentialType::Login,
        is_favorite: false,
        expires_at: None,
        expiry_status: ExpiryStatus::None,
        tags: Vec::new(),
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        fields: Vec::new(),
        password_strength: None,
        deleted_at: None,
    });
    assert!(state.detail.record.is_some());

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let _result = state.update(Message::KeyEvent(key_event(KeyCode::Char('j'))), &mut ctx);

    // Detail should be cleared after sidebar filter change
    assert!(state.detail.record.is_none());
    assert!(!state.detail.password_visible);
}

#[test]
fn sidebar_j_exits_visual_mode() {
    let mut state = MainScreenState {
        focused_panel: PanelId::Sidebar,
        ..Default::default()
    };
    state.list.enter_visual();
    assert!(state.list.is_visual());

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let _result = state.update(Message::KeyEvent(key_event(KeyCode::Char('j'))), &mut ctx);

    assert!(!state.list.is_visual());
}

#[test]
fn sidebar_j_no_filter_change_does_not_reload() {
    // Navigating between items with the same filter (e.g., Config -> All)
    // should not trigger a reload.
    let mut state = MainScreenState {
        focused_panel: PanelId::Sidebar,
        current_filter: RecordFilter::All,
        ..Default::default()
    };

    // Select Generator (which also returns RecordFilter::All)
    let gen_idx = state
        .sidebar
        .items
        .iter()
        .position(|i| matches!(i, SidebarItem::Generator))
        .expect("Generator should be in sidebar");
    state.sidebar.selected_index = gen_idx;

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(Message::KeyEvent(key_event(KeyCode::Char('j'))), &mut ctx);

    // Should NOT be a Command since filter didn't change (Generator -> All, both are All)
    assert!(matches!(result, ScreenResult::Continue));
    assert!(!state.list_auto_select);
}

// ── list_auto_select -> RecordListLoaded auto-select tests ─────────────────

#[test]
fn record_list_loaded_auto_selects_first_when_flag_is_true() {
    use crate::commands::result::CommandResult;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    let records = vec![
        TuiRecord {
            id: Uuid::new_v4(),
            credential_type: CredentialType::Login,
            name: "Record 1".to_string(),
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
        },
        TuiRecord {
            id: Uuid::new_v4(),
            credential_type: CredentialType::Login,
            name: "Record 2".to_string(),
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
        },
    ];

    let mut state = MainScreenState {
        list_auto_select: true,
        ..Default::default()
    };

    let (tx, mut rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: records.clone(),
            total: 2,
            category_counts: crate::commands::types::RecordCategoryCounts::default(),
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.list.records.len(), 2);
    assert_eq!(state.list.selected_index, Some(0));
    // Flag should be reset after consumption
    assert!(!state.list_auto_select);

    // Should send LoadRecordDetail for the first record
    let cmd = rx.try_recv().expect("Should send LoadRecordDetail command");
    match cmd {
        Command::LoadRecordDetail { id } => {
            assert_eq!(id, records[0].id);
        }
        other => panic!("Expected LoadRecordDetail, got {:?}", other),
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn record_list_loaded_auto_select_handles_empty_list() {
    use crate::commands::result::CommandResult;

    let mut state = MainScreenState::default();
    state.list_auto_select = true;
    // Set up a detail record so we can verify it gets cleared on empty list
    state.detail.record = Some(crate::tui::state::detail_state::DetailViewData {
        id: Uuid::new_v4(),
        name: "Test".to_string(),
        subtitle: String::new(),
        credential_type: crate::types::credential::CredentialType::Login,
        is_favorite: false,
        expires_at: None,
        expiry_status: crate::tui::state::detail_state::ExpiryStatus::None,
        tags: Vec::new(),
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        fields: Vec::new(),
        password_strength: None,
        deleted_at: None,
    });
    assert!(state.detail.record.is_some());

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: Vec::new(),
            total: 0,
            category_counts: crate::commands::types::RecordCategoryCounts::default(),
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.list.records.is_empty());
    assert_eq!(state.list.selected_index, None);
    // Flag should be reset
    assert!(!state.list_auto_select);
    // Detail should be cleared for empty list
    assert!(state.detail.record.is_none());
}

#[test]
fn record_list_loaded_does_not_auto_select_when_flag_is_false() {
    use crate::commands::result::CommandResult;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    let records = vec![TuiRecord {
        id: Uuid::new_v4(),
        credential_type: CredentialType::Login,
        name: "Record".to_string(),
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
    }];

    let mut state = MainScreenState::default();
    // list_auto_select is false by default (initial load)

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: records.clone(),
            total: 1,
            category_counts: crate::commands::types::RecordCategoryCounts::default(),
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.list.records.len(), 1);
    // Should NOT auto-select — selected_index stays None (U4 Empty State)
    assert_eq!(state.list.selected_index, None);
}

// ── Search mode tests ─────────────────────────────────────────────────────

#[test]
fn search_mode_g_does_not_navigate_to_config() {
    use crate::commands::types::PanelId;

    use crate::tui::state::list_state::ListPanelState;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    fn make_test_record(name: &str) -> TuiRecord {
        TuiRecord {
            id: uuid::Uuid::new_v4(),
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

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let records = vec![make_test_record("Test")];
    let mut state = MainScreenState {
        list: ListPanelState::with_records(records),
        focused_panel: PanelId::List,
        ..Default::default()
    };
    state.list.enter_search();

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    // Press 'g' — should NOT navigate to config
    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('g'))), &mut ctx);
    // Result should be Continue, not NavigateTo(Config)
    assert!(matches!(result, ScreenResult::Continue));
    // Search query should contain 'g'
    if let crate::tui::state::list_state::ListMode::Search(ref s) = state.list.mode {
        assert_eq!(s.query, "g");
    } else {
        panic!("Expected search mode");
    }
}

#[test]
fn ctrl_k_enters_search_mode_in_state_update() {
    use crate::commands::types::PanelId;
    use crate::tui::state::list_state::ListPanelState;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    fn make_test_record(name: &str) -> TuiRecord {
        TuiRecord {
            id: uuid::Uuid::new_v4(),
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

    let records = vec![make_test_record("GitHub"), make_test_record("GitLab")];
    let mut state = MainScreenState {
        list: ListPanelState::with_records(records),
        focused_panel: PanelId::List,
        ..Default::default()
    };
    state.list.selected_index = Some(1);

    let mut ctx = make_ctx();
    let result = state.update(
        Message::KeyEvent(key_event_with_modifiers(
            KeyCode::Char('k'),
            KeyModifiers::CONTROL,
        )),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.list.is_searching());
    assert_eq!(state.list.selected_index, Some(1));
}

#[test]
fn search_mode_typing_updates_query() {
    use crate::commands::types::PanelId;

    use crate::tui::state::list_state::ListPanelState;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    fn make_test_record(name: &str) -> TuiRecord {
        TuiRecord {
            id: uuid::Uuid::new_v4(),
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

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let records = vec![make_test_record("Test")];
    let mut state = MainScreenState {
        list: ListPanelState::with_records(records),
        focused_panel: PanelId::List,
        ..Default::default()
    };
    state.list.enter_search();

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    // Type 'abc'
    for c in ['a', 'b', 'c'] {
        state.update(Message::KeyEvent(make_key(KeyCode::Char(c))), &mut ctx);
    }

    if let crate::tui::state::list_state::ListMode::Search(ref s) = state.list.mode {
        assert_eq!(s.query, "abc");
    } else {
        panic!("Expected search mode");
    }
}

#[test]
fn search_mode_backspace_removes_last_char() {
    use crate::commands::types::PanelId;

    use crate::tui::state::list_state::ListPanelState;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    fn make_test_record(name: &str) -> TuiRecord {
        TuiRecord {
            id: uuid::Uuid::new_v4(),
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

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let records = vec![make_test_record("Test")];
    let mut state = MainScreenState {
        list: ListPanelState::with_records(records),
        focused_panel: PanelId::List,
        ..Default::default()
    };
    state.list.enter_search();

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    // Type 'abc' then backspace
    for c in ['a', 'b', 'c'] {
        state.update(Message::KeyEvent(make_key(KeyCode::Char(c))), &mut ctx);
    }
    state.update(Message::KeyEvent(make_key(KeyCode::Backspace)), &mut ctx);

    if let crate::tui::state::list_state::ListMode::Search(ref s) = state.list.mode {
        assert_eq!(s.query, "ab");
    } else {
        panic!("Expected search mode");
    }
}

#[test]
fn search_mode_backspace_unicode() {
    use crate::commands::types::PanelId;
    use crate::tui::state::list_state::ListPanelState;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    fn make_test_record(name: &str) -> TuiRecord {
        TuiRecord {
            id: uuid::Uuid::new_v4(),
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

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let records = vec![make_test_record("Test")];
    let mut state = MainScreenState {
        list: ListPanelState::with_records(records),
        focused_panel: PanelId::List,
        ..Default::default()
    };
    state.list.enter_search();

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    // Type "你好" then backspace — should leave "你"
    for c in ['你', '好'] {
        state.update(Message::KeyEvent(make_key(KeyCode::Char(c))), &mut ctx);
    }
    state.update(Message::KeyEvent(make_key(KeyCode::Backspace)), &mut ctx);

    if let crate::tui::state::list_state::ListMode::Search(ref s) = state.list.mode {
        assert_eq!(s.query, "你");
    } else {
        panic!("Expected search mode");
    }
}

#[test]
fn search_mode_esc_exits_search() {
    use crate::commands::types::PanelId;

    use crate::tui::state::list_state::ListPanelState;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    fn make_test_record(name: &str) -> TuiRecord {
        TuiRecord {
            id: uuid::Uuid::new_v4(),
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

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let records = vec![make_test_record("Test")];
    let mut state = MainScreenState {
        list: ListPanelState::with_records(records),
        focused_panel: PanelId::List,
        ..Default::default()
    };
    state.list.enter_search();

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    // Type something then press Esc
    state.update(Message::KeyEvent(make_key(KeyCode::Char('x'))), &mut ctx);
    assert!(state.list.is_searching());

    state.update(Message::KeyEvent(make_key(KeyCode::Esc)), &mut ctx);
    assert!(!state.list.is_searching());
}

// ── Layer 2 key handling tests ─────────────────────────────────────────────

#[test]
fn e_from_list_navigates_to_edit() {
    use crate::commands::types::PanelId;
    use crate::tui::state::list_state::ListPanelState;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    fn make_test_record(name: &str) -> TuiRecord {
        TuiRecord {
            id: uuid::Uuid::new_v4(),
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

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let records = vec![make_test_record("Test")];
    let mut state = MainScreenState {
        list: ListPanelState::with_records(records),
        focused_panel: PanelId::List,
        ..Default::default()
    };

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    // Press 'e' — should navigate to EditRecord screen
    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('e'))), &mut ctx);
    assert!(matches!(result, ScreenResult::NavigateTo(_)));
}

// ── Helpers ─────────────────────────────────────────────────────────────────

use crate::tui::state::detail_state::{DetailViewData, ExpiryStatus};
use crate::types::credential::CredentialType;
use crate::types::record::TuiRecord;

fn make_test_record(id_override: Option<Uuid>) -> TuiRecord {
    TuiRecord {
        id: id_override.unwrap_or_else(Uuid::new_v4),
        credential_type: CredentialType::Login,
        name: "Test Record".to_string(),
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

#[allow(dead_code)]
fn make_test_record_with_id(id: Uuid) -> TuiRecord {
    TuiRecord {
        id,
        credential_type: CredentialType::Login,
        name: "Test Record".to_string(),
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

fn make_detail_view(id: Uuid, is_favorite: bool) -> DetailViewData {
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
        fields: Vec::new(),
        password_strength: None,
        deleted_at: None,
    }
}

// ── Record CRUD refresh tests ────────────────────────────────

#[test]
fn record_created_triggers_list_refresh() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::All,
        ..Default::default()
    };
    assert!(!state.list_auto_select);

    let (tx, mut rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordCreated { id: record_id }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    // list_auto_select should be set so next RecordListLoaded auto-selects first record
    assert!(state.list_auto_select);

    // Should send LoadRecordList
    let cmd = rx
        .try_recv()
        .expect("Should send LoadRecordList command after RecordCreated");
    match cmd {
        Command::LoadRecordList { filter, .. } => {
            assert_eq!(filter, RecordFilter::All);
        }
        other => panic!("Expected LoadRecordList, got {:?}", other),
    }
}

#[test]
fn record_updated_triggers_list_refresh() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::All,
        ..Default::default()
    };

    let (tx, mut rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordUpdated { id: record_id }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));

    // Should send LoadRecordList
    let cmd = rx
        .try_recv()
        .expect("Should send LoadRecordList command after RecordUpdated");
    match cmd {
        Command::LoadRecordList { filter, .. } => {
            assert_eq!(filter, RecordFilter::All);
        }
        other => panic!("Expected LoadRecordList, got {:?}", other),
    }
}

#[test]
fn record_updated_refreshes_detail_when_showing() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::All,
        ..Default::default()
    };
    state.list.records = vec![make_test_record(Some(record_id))];
    state.list.selected_index = Some(0);
    state.detail.record = Some(make_detail_view(record_id, false));
    assert!(state.detail.record.is_some());
    assert_eq!(state.detail.record.as_ref().unwrap().id, record_id);

    let (tx, mut rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let _result = state.update(
        Message::CommandCompleted(CommandResult::RecordUpdated { id: record_id }),
        &mut ctx,
    );

    // Only LoadRecordList is sent immediately; detail refresh is deferred
    let cmd1 = rx.try_recv().expect("Should send LoadRecordList");
    assert!(matches!(cmd1, Command::LoadRecordList { .. }));

    // No immediate LoadRecordDetail
    assert!(
        rx.try_recv().is_err(),
        "Should NOT send LoadRecordDetail immediately (deferred to RecordListLoaded)"
    );

    // pending_detail_refresh is set for the deferred load
    assert_eq!(state.pending_detail_refresh, Some(record_id));
}

#[test]
fn record_updated_does_not_refresh_detail_when_record_no_longer_in_filtered_list() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::All,
        ..Default::default()
    };
    state.list.records = vec![make_test_record(Some(record_id))];
    state.list.selected_index = Some(0);
    state.detail.record = Some(make_detail_view(record_id, false));
    assert!(state.detail.record.is_some());
    assert_eq!(state.detail.record.as_ref().unwrap().id, record_id);

    let (tx, mut rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    // Send RecordUpdated
    let _result = state.update(
        Message::CommandCompleted(CommandResult::RecordUpdated { id: record_id }),
        &mut ctx,
    );

    // Drain LoadRecordList
    let _cmd = rx.try_recv().expect("Should send LoadRecordList");

    // Now simulate a list reload where the record is no longer present
    // (e.g. editing made it not match the filter)
    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: vec![], // empty — record filtered out
            total: 0,
            category_counts: crate::commands::types::RecordCategoryCounts::default(),
        }),
        &mut ctx,
    );
    assert!(matches!(result, ScreenResult::Continue));

    // Detail should be cleared since the record is no longer in the list
    assert!(state.detail.record.is_none());
    assert_eq!(state.pending_detail_refresh, None);
}

#[test]
fn record_updated_does_not_refresh_detail_when_showing_different_record() {
    use crate::commands::result::CommandResult;

    let updated_id = Uuid::new_v4();
    let detail_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::All,
        ..Default::default()
    };
    state.list.records = vec![
        make_test_record(Some(updated_id)),
        make_test_record(Some(detail_id)),
    ];
    state.list.selected_index = Some(1);
    state.detail.record = Some(make_detail_view(detail_id, false));
    assert!(state.detail.record.is_some());
    assert_eq!(state.detail.record.as_ref().unwrap().id, detail_id);

    let (tx, mut rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordUpdated { id: updated_id }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));

    // Should send LoadRecordList
    let cmd = rx
        .try_recv()
        .expect("Should send LoadRecordList after RecordUpdated");
    assert!(matches!(cmd, Command::LoadRecordList { .. }));

    // Should NOT send LoadRecordDetail (detail was showing a different record)
    assert!(
        rx.try_recv().is_err(),
        "Should not send LoadRecordDetail when detail shows a different record"
    );

    // Detail should still be showing the original record
    assert!(state.detail.record.is_some());
    assert_eq!(state.detail.record.as_ref().unwrap().id, detail_id);
}

#[test]
fn record_deleted_clears_detail_when_showing() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::All,
        ..Default::default()
    };
    state.list.records = vec![make_test_record(Some(record_id))];
    state.list.selected_index = Some(0);
    state.detail.record = Some(make_detail_view(record_id, false));
    assert!(state.detail.record.is_some());

    let (tx, mut rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordDeleted { id: record_id }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    // Detail should be cleared because it was showing the deleted record
    assert!(state.detail.record.is_none());

    // Should send LoadRecordList
    let cmd = rx
        .try_recv()
        .expect("Should send LoadRecordList after RecordDeleted");
    assert!(matches!(cmd, Command::LoadRecordList { .. }));
}

#[test]
fn record_deleted_does_not_clear_detail_when_not_showing() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let other_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::All,
        ..Default::default()
    };
    state.list.records = vec![make_test_record(Some(record_id))];
    state.list.selected_index = Some(0);
    state.detail.record = Some(make_detail_view(other_id, false));
    assert!(state.detail.record.is_some());

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordDeleted { id: record_id }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    // Detail should NOT be cleared because it was showing a different record
    assert!(state.detail.record.is_some());
}

#[test]
fn record_restored_triggers_list_refresh() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::Trash,
        ..Default::default()
    };

    let (tx, mut rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordRestored { id: record_id }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));

    // Should send LoadRecordList
    let cmd = rx
        .try_recv()
        .expect("Should send LoadRecordList after RecordRestored");
    assert!(matches!(cmd, Command::LoadRecordList { .. }));
}

#[test]
fn record_restored_clears_detail_when_showing() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::Trash,
        ..Default::default()
    };
    state.list.records = vec![make_test_record(Some(record_id))];
    state.list.selected_index = Some(0);
    state.detail.record = Some(make_detail_view(record_id, false));
    assert!(state.detail.record.is_some());

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordRestored { id: record_id }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    // Detail should be cleared because it was showing the restored record
    assert!(state.detail.record.is_none());
}

#[test]
fn record_destroyed_triggers_list_refresh() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::Trash,
        ..Default::default()
    };

    let (tx, mut rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordDestroyed { id: record_id }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));

    // Should send LoadRecordList
    let cmd = rx
        .try_recv()
        .expect("Should send LoadRecordList after RecordDestroyed");
    assert!(matches!(cmd, Command::LoadRecordList { .. }));
}

// ── Cursor recovery tests ────────────────────────────────────

#[test]
#[allow(clippy::field_reassign_with_default)]
fn record_list_loaded_cursor_recovery_keeps_selection_by_id() {
    use crate::commands::result::CommandResult;

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.list.records = vec![make_test_record(Some(id1)), make_test_record(Some(id2))];
    state.list.selected_index = Some(1);
    state.list_auto_select = false;

    let (tx, mut rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    // New records in different order — id2 is now at index 0
    let new_records = vec![make_test_record(Some(id2)), make_test_record(Some(id1))];
    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: new_records,
            total: 2,
            category_counts: crate::commands::types::RecordCategoryCounts::default(),
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    // Selection should follow record id2 (was at index 1, now at index 0)
    assert_eq!(state.list.selected_index, Some(0));
    assert!(!state.list_auto_select);
    // No LoadRecordDetail sent — same record id remains selected
    assert!(rx.try_recv().is_err());
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn record_list_loaded_cursor_recovery_falls_back_when_id_disappears() {
    use crate::commands::result::CommandResult;

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.list.records = vec![
        make_test_record(Some(id1)),
        make_test_record(Some(id2)),
        make_test_record(Some(id3)),
    ];
    state.list.selected_index = Some(2); // selected id3
    state.list_auto_select = false;

    let (tx, mut rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    // New list does not contain id3 — fallback to first row (id1)
    let new_records = vec![make_test_record(Some(id1))];
    let _result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: new_records,
            total: 1,
            category_counts: crate::commands::types::RecordCategoryCounts::default(),
        }),
        &mut ctx,
    );

    assert!(matches!(_result, ScreenResult::Continue));
    // Fallback to first row when selected id is gone
    assert_eq!(state.list.selected_index, Some(0));
    // LoadRecordDetail should be sent for the new selection (different id)
    let cmd = rx
        .try_recv()
        .expect("Should send LoadRecordDetail for fallback record");
    match cmd {
        Command::LoadRecordDetail { id } => {
            assert_eq!(id, id1);
        }
        other => panic!("Expected LoadRecordDetail, got {:?}", other),
    }
}

#[test]
fn record_list_loaded_cursor_recovery_initial_load() {
    use crate::commands::result::CommandResult;

    let mut state = MainScreenState {
        list_auto_select: false,
        ..Default::default()
    };
    // Initial state: selected_index = None, no records
    assert_eq!(state.list.selected_index, None);

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let new_records = vec![make_test_record(None)];
    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: new_records,
            total: 1,
            category_counts: crate::commands::types::RecordCategoryCounts::default(),
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    // Initial load should keep None for U4 Empty State
    assert_eq!(state.list.selected_index, None);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn record_list_loaded_cursor_recovery_keeps_none_when_empty_list() {
    use crate::commands::result::CommandResult;

    let mut state = MainScreenState::default();
    state.list.selected_index = Some(0);
    state.list_auto_select = false;

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: Vec::new(),
            total: 0,
            category_counts: crate::commands::types::RecordCategoryCounts::default(),
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.list.selected_index, None);
    assert!(state.detail.record.is_none()); // detail cleared for empty list
}

// ── FavoriteToggled tests ────────────────────────────────────

#[test]
#[allow(clippy::field_reassign_with_default)]
fn favorite_toggled_updates_detail_is_favorite() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::All,
        ..Default::default()
    };
    state.list.records = vec![make_test_record(Some(record_id))];
    state.detail.record = Some(make_detail_view(record_id, false));
    assert!(!state.detail.record.as_ref().unwrap().is_favorite);

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::FavoriteToggled {
            id: record_id,
            is_favorite: true,
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.detail.record.as_ref().unwrap().is_favorite);
    assert!(state.list.records[0].is_favorite);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn favorite_toggled_does_not_update_detail_when_not_showing() {
    use crate::commands::result::CommandResult;

    let toggled_id = Uuid::new_v4();
    let detail_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::All,
        ..Default::default()
    };
    state.list.records = vec![
        make_test_record(Some(toggled_id)),
        make_test_record(Some(detail_id)),
    ];
    state.detail.record = Some(make_detail_view(detail_id, false));

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::FavoriteToggled {
            id: toggled_id,
            is_favorite: true,
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    // Detail record's is_favorite should NOT be updated (it's a different record)
    assert!(!state.detail.record.as_ref().unwrap().is_favorite);
    // List record's is_favorite should be updated
    assert!(state.list.records[0].is_favorite);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn favorite_toggled_removes_from_list_when_viewing_favorites() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState {
        current_filter: RecordFilter::Favorites,
        ..Default::default()
    };
    state.list.records = vec![make_test_record(Some(record_id))];
    state.list.selected_index = Some(0);
    state.detail.record = Some(make_detail_view(record_id, true));
    assert_eq!(state.list.records.len(), 1);

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::FavoriteToggled {
            id: record_id,
            is_favorite: false,
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    // Record should be removed from list when unfavoriting while viewing Favorites
    assert!(state.list.records.is_empty());
    // selected_index should be None since list is empty
    assert_eq!(state.list.selected_index, None);
    // Detail record's is_favorite should still be updated
    assert!(!state.detail.record.as_ref().unwrap().is_favorite);
}

// ── RecordDetailLoaded tests ─────────────────────────────────

#[test]
fn record_detail_loaded_populates_detail_panel_with_strength_and_health() {
    use crate::commands::result::CommandResult;
    use crate::commands::types::HealthIssue;
    use crate::crypto::strength::{PasswordStrength as CryptoStrength, StrengthLevel};
    use crate::tui::state::detail_state::PasswordStrength as DetailStrength;
    use crate::types::record::DecryptedRecord;
    use crate::types::sensitive::SecureStr;

    let record = DecryptedRecord::Login {
        id: Uuid::new_v4(),
        name: "Test Record".into(),
        username: "user".into(),
        password: SecureStr::new("pass".into()),
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

    let strength = CryptoStrength {
        level: StrengthLevel::Strong,
        char_types: 4,
        bar_fill: 12,
    };
    let health_issue = HealthIssue::Weak;

    let mut state = MainScreenState::default();
    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordDetailLoaded {
            record,
            password_strength: Some(strength),
            health_issue: Some(health_issue),
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.detail.record.is_some());
    assert_eq!(
        state.detail.record.as_ref().unwrap().password_strength,
        Some(DetailStrength::Strong)
    );
    assert_eq!(state.detail.health_issue, Some(HealthIssue::Weak));
    assert_eq!(state.detail.record.as_ref().unwrap().name, "Test Record");
    assert!(!state.detail.password_visible);
    assert_eq!(state.detail.focused_field, 0);
}

// ── Overlay result dispatch tests ────────────────────────────

#[test]
fn restore_confirm_dispatches_command() {
    let record_id = uuid::Uuid::new_v4();
    let mut state = MainScreenState::default();
    let dialog = ConfirmDialogState {
        variant: ConfirmVariant::Restore {
            record_id,
            record_name: "Test Record".to_string(),
        },
        focused_button: ConfirmButton::Confirm,
    };
    assert!(state.overlay_manager.open(Overlay::ConfirmDialog(dialog)));
    assert!(state.overlay_manager.is_active());

    // Confirm the restore dialog
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };
    let result = state.update(Message::KeyEvent(key), &mut ctx);

    assert!(!state.overlay_manager.is_active());
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::RestoreRecord { id } if *id == record_id => {}
            other => panic!(
                "Expected RestoreRecord with id {:?}, got {:?}",
                record_id, other
            ),
        }
    } else {
        panic!("Expected Command result, got {:?}", result);
    }
}

// ── Task 6: List normal mode j/k navigation tests ────────────────────────────

#[test]
fn j_in_list_normal_mode_moves_down_and_loads_detail() {
    use crate::commands::types::PanelId;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    fn make_test_record() -> TuiRecord {
        TuiRecord {
            id: uuid::Uuid::new_v4(),
            credential_type: CredentialType::Login,
            name: "Test".to_string(),
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

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let records: Vec<TuiRecord> = (0..3).map(|_| make_test_record()).collect();
    let mut state = MainScreenState {
        list: crate::tui::state::list_state::ListPanelState::with_records(records),
        focused_panel: PanelId::List,
        ..Default::default()
    };

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    // Press j — should move down to index 1
    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('j'))), &mut ctx);
    assert_eq!(state.list.selected_index, Some(1));

    assert!(matches!(result, ScreenResult::Command(_)));
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::LoadRecordDetail { id } => {
                assert_eq!(*id, state.list.records[1].id);
            }
            other => panic!("Expected LoadRecordDetail, got {:?}", other),
        }
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn k_in_list_normal_mode_moves_up_and_loads_detail() {
    use crate::commands::types::PanelId;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    fn make_test_record() -> TuiRecord {
        TuiRecord {
            id: uuid::Uuid::new_v4(),
            credential_type: CredentialType::Login,
            name: "Test".to_string(),
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

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let records: Vec<TuiRecord> = (0..3).map(|_| make_test_record()).collect();
    let mut state = MainScreenState::default();
    state.list = crate::tui::state::list_state::ListPanelState::with_records(records);
    state.list.selected_index = Some(2);
    state.focused_panel = PanelId::List;

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    // Press k — should move up to index 1
    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('k'))), &mut ctx);
    assert_eq!(state.list.selected_index, Some(1));

    assert!(matches!(result, ScreenResult::Command(_)));
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::LoadRecordDetail { id } => {
                assert_eq!(*id, state.list.records[1].id);
            }
            other => panic!("Expected LoadRecordDetail, got {:?}", other),
        }
    }
}

#[test]
fn jk_in_list_normal_mode_handles_empty_list() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let mut state = MainScreenState {
        focused_panel: PanelId::List,
        ..Default::default()
    };

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    // Press j with empty list — should be Continue (no command)
    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('j'))), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.list.selected_index, None);

    // Press k with empty list — should be Continue (no command)
    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('k'))), &mut ctx);
    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.list.selected_index, None);
}

// ── Task 7: Detail panel keyboard shortcut tests ─────────────────────────────

fn make_detail_view_with_fields(id: Uuid, is_favorite: bool) -> DetailViewData {
    use crate::tui::state::detail_state::{DetailField, DetailFieldKind, FieldValue};
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
        ],
        password_strength: None,
        deleted_at: None,
    }
}

fn make_ssh_detail_view(id: Uuid, is_favorite: bool) -> DetailViewData {
    use crate::tui::state::detail_state::{DetailField, DetailFieldKind, FieldValue};
    DetailViewData {
        id,
        name: "SSH Key".to_string(),
        subtitle: String::new(),
        credential_type: CredentialType::Ssh,
        is_favorite,
        expires_at: None,
        expiry_status: ExpiryStatus::None,
        tags: Vec::new(),
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        fields: vec![
            DetailField {
                label: "Public Key".to_string(),
                value: FieldValue::Plain("ssh-rsa AAA...".to_string()),
                copyable: true,
                toggleable: false,
                kind: DetailFieldKind::PublicKey,
            },
            DetailField {
                label: "Private Key".to_string(),
                value: FieldValue::Masked,
                copyable: true,
                toggleable: true,
                kind: DetailFieldKind::PrivateKey,
            },
            DetailField {
                label: "Passphrase".to_string(),
                value: FieldValue::Masked,
                copyable: true,
                toggleable: true,
                kind: DetailFieldKind::Passphrase,
            },
        ],
        password_strength: None,
        deleted_at: None,
    }
}

fn make_secure_note_detail_view(id: Uuid, is_favorite: bool) -> DetailViewData {
    use crate::tui::state::detail_state::{DetailField, DetailFieldKind, FieldValue};
    DetailViewData {
        id,
        name: "Secure Note".to_string(),
        subtitle: String::new(),
        credential_type: CredentialType::SecureNote,
        is_favorite,
        expires_at: None,
        expiry_status: ExpiryStatus::None,
        tags: Vec::new(),
        notes: Some("private note".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        fields: vec![DetailField {
            label: "Notes".to_string(),
            value: FieldValue::Plain("private note".to_string()),
            copyable: false,
            toggleable: false,
            kind: DetailFieldKind::Notes,
        }],
        password_strength: None,
        deleted_at: None,
    }
}

#[test]
fn ssh_passphrase_maps_to_passphrase_selector() {
    use crate::tui::state::detail_state::DetailFieldKind;
    assert_eq!(
        detail_field_kind_to_selector(DetailFieldKind::Passphrase),
        FieldSelector::Passphrase
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn field_decrypted_updates_correct_field() {
    use crate::commands::result::CommandResult;
    use crate::tui::state::detail_state::{DetailFieldKind, FieldValue};

    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.detail.record = Some(make_ssh_detail_view(id, false));

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    // Send FieldDecrypted with FieldSelector::Passphrase
    let value = SecureStr::new("my_ssh_passphrase".to_string());
    let result = state.update(
        Message::CommandCompleted(CommandResult::FieldDecrypted {
            id,
            field: FieldSelector::Passphrase,
            value,
        }),
        &mut ctx,
    );
    assert!(matches!(result, ScreenResult::Continue));

    // Verify Passphrase field was revealed, but PrivateKey stayed masked
    let fields = &state.detail.record.as_ref().unwrap().fields;
    let private_key = fields
        .iter()
        .find(|f| f.kind == DetailFieldKind::PrivateKey)
        .unwrap();
    assert!(matches!(private_key.value, FieldValue::Masked));
    let passphrase = fields
        .iter()
        .find(|f| f.kind == DetailFieldKind::Passphrase)
        .unwrap();
    assert!(matches!(passphrase.value, FieldValue::Revealed(ref v) if v == "my_ssh_passphrase"));
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn p_on_detail_sends_decrypt_field() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(id, false));

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('p'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Command(_)));
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::DecryptField { id: cmd_id, field } => {
                assert_eq!(*cmd_id, id);
                assert_eq!(*field, FieldSelector::Password);
            }
            other => panic!("Expected DecryptField, got {:?}", other),
        }
    }
    // toggle_password() was called which sets password_visible = true
    assert!(state.detail.password_visible);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn p_on_focused_passphrase_sends_decrypt_passphrase() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState {
        focused_panel: PanelId::Detail,
        ..Default::default()
    };
    state.detail.record = Some(make_ssh_detail_view(id, false));
    // Focus on Passphrase field (index 2 in SSH detail: PublicKey=0, PrivateKey=1, Passphrase=2)
    state.detail.focused_field = 2;

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('p'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Command(_)));
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::DecryptField { id: cmd_id, field } => {
                assert_eq!(*cmd_id, id);
                assert_eq!(*field, FieldSelector::Passphrase);
            }
            other => panic!("Expected DecryptField(Passphrase), got {:?}", other),
        }
    }
    assert!(state.detail.password_visible);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn p_on_ssh_detail_reveals_all_hidden_fields() {
    use crate::commands::result::CommandResult;
    use crate::commands::types::PanelId;
    use crate::tui::state::detail_state::{DetailFieldKind, FieldValue};

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState {
        focused_panel: PanelId::Detail,
        ..Default::default()
    };
    state.detail.record = Some(make_ssh_detail_view(id, false));

    let (tx, mut rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('p'))), &mut ctx);
    match result {
        ScreenResult::Command(cmd) => match *cmd {
            Command::DecryptField { id: cmd_id, field } => {
                assert_eq!(cmd_id, id);
                assert_eq!(field, FieldSelector::Password);
            }
            other => panic!("Expected DecryptField(Password), got {other:?}"),
        },
        other => panic!("Expected command result, got {other:?}"),
    }

    let result = state.update(
        Message::CommandCompleted(CommandResult::FieldDecrypted {
            id,
            field: FieldSelector::Password,
            value: SecureStr::new("private-key".to_string()),
        }),
        &mut ctx,
    );
    assert!(matches!(result, ScreenResult::Continue));
    match rx.try_recv().expect("passphrase decrypt should be queued") {
        Command::DecryptField { id: cmd_id, field } => {
            assert_eq!(cmd_id, id);
            assert_eq!(field, FieldSelector::Passphrase);
        }
        other => panic!("Expected queued DecryptField(Passphrase), got {other:?}"),
    }

    let result = state.update(
        Message::CommandCompleted(CommandResult::FieldDecrypted {
            id,
            field: FieldSelector::Passphrase,
            value: SecureStr::new("passphrase".to_string()),
        }),
        &mut ctx,
    );
    assert!(matches!(result, ScreenResult::Continue));

    let fields = &state.detail.record.as_ref().expect("detail").fields;
    let private_key = fields
        .iter()
        .find(|f| f.kind == DetailFieldKind::PrivateKey)
        .expect("private key");
    assert!(matches!(private_key.value, FieldValue::Revealed(ref v) if v == "private-key"));
    let passphrase = fields
        .iter()
        .find(|f| f.kind == DetailFieldKind::Passphrase)
        .expect("passphrase");
    assert!(matches!(passphrase.value, FieldValue::Revealed(ref v) if v == "passphrase"));
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn secure_note_detail_shortcuts_do_not_copy_or_toggle() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState {
        focused_panel: PanelId::Detail,
        ..Default::default()
    };
    state.detail.record = Some(make_secure_note_detail_view(id, false));

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    for key in ['c', 'u', 'p'] {
        let result = state.update(Message::KeyEvent(make_key(KeyCode::Char(key))), &mut ctx);
        assert!(
            matches!(result, ScreenResult::Continue),
            "{key} should not trigger a Secure Note copy/toggle command"
        );
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn field_decrypted_passphrase_only_reveals_passphrase_not_private_key() {
    use crate::commands::result::CommandResult;
    use crate::tui::state::detail_state::{DetailFieldKind, FieldValue};

    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.detail.record = Some(make_ssh_detail_view(id, false));
    // Focus on Passphrase field (index 2)
    state.detail.focused_field = 2;

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let value = SecureStr::new("ssh_passphrase_value".to_string());
    let result = state.update(
        Message::CommandCompleted(CommandResult::FieldDecrypted {
            id,
            field: FieldSelector::Passphrase,
            value,
        }),
        &mut ctx,
    );
    assert!(matches!(result, ScreenResult::Continue));

    // Verify Passphrase field was revealed, but PrivateKey stayed masked
    let fields = &state.detail.record.as_ref().unwrap().fields;
    let private_key = fields
        .iter()
        .find(|f| f.kind == DetailFieldKind::PrivateKey)
        .unwrap();
    assert!(matches!(private_key.value, FieldValue::Masked));
    let passphrase = fields
        .iter()
        .find(|f| f.kind == DetailFieldKind::Passphrase)
        .unwrap();
    assert!(matches!(passphrase.value, FieldValue::Revealed(ref v) if v == "ssh_passphrase_value"));
}

#[test]
fn p_on_detail_without_record_opens_generator() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = None;

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let _result = state.update(Message::KeyEvent(make_key(KeyCode::Char('p'))), &mut ctx);

    // Should still open password generator (fallthrough to Layer 2)
    assert!(state.overlay_manager.is_active());
}

#[test]
fn c_on_detail_copies_password() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(id, false));

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('c'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Command(_)));
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::CopyToClipboard {
                id: cmd_id,
                field: FieldSelector::Password,
            } => {
                assert_eq!(*cmd_id, id);
            }
            other => panic!("Expected CopyToClipboard(Password), got {:?}", other),
        }
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn u_on_detail_copies_username() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(id, false));

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('u'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Command(_)));
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::CopyToClipboard {
                id: cmd_id,
                field: FieldSelector::Username,
            } => {
                assert_eq!(*cmd_id, id);
            }
            other => panic!("Expected CopyToClipboard(Username), got {:?}", other),
        }
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn enter_on_detail_copies_current_field() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState {
        focused_panel: PanelId::Detail,
        ..Default::default()
    };
    state.detail.record = Some(make_detail_view_with_fields(id, false));
    state.detail.focused_field = 0; // Username

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(Message::KeyEvent(make_key(KeyCode::Enter)), &mut ctx);

    assert!(matches!(result, ScreenResult::Command(_)));
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::CopyToClipboard {
                id: cmd_id,
                field: FieldSelector::Username,
            } => {
                assert_eq!(*cmd_id, id);
            }
            other => panic!("Expected CopyToClipboard(Username), got {:?}", other),
        }
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn f_on_detail_toggles_favorite() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(id, false));
    assert!(!state.detail.record.as_ref().unwrap().is_favorite);

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('f'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Command(_)));
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::ToggleFavorite {
                id: cmd_id,
                is_favorite,
            } => {
                assert_eq!(*cmd_id, id);
                assert!(*is_favorite);
            }
            other => panic!("Expected ToggleFavorite, got {:?}", other),
        }
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn d_on_detail_opens_delete_confirm() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(id, false));

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let _result = state.update(Message::KeyEvent(make_key(KeyCode::Char('d'))), &mut ctx);

    assert!(state.overlay_manager.is_active());
    match state.overlay_manager.get() {
        Some(crate::tui::screens::main::overlay::ActiveOverlay::ConfirmDialog {
            variant, ..
        }) => {
            assert!(matches!(variant, ConfirmVariant::SoftDelete { .. }));
        }
        other => panic!("Expected ConfirmDialog overlay, got {:?}", other),
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn h_on_detail_loads_password_history() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(id, false));

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(Message::KeyEvent(make_key(KeyCode::Char('H'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Command(_)));
    if let ScreenResult::Command(cmd) = result {
        match &*cmd {
            Command::LoadPasswordHistory { record_id } => {
                assert_eq!(*record_id, id);
            }
            other => panic!("Expected LoadPasswordHistory, got {:?}", other),
        }
    }
}

#[test]
fn j_on_detail_moves_field_down() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState {
        focused_panel: PanelId::Detail,
        ..Default::default()
    };
    state.detail.record = Some(make_detail_view_with_fields(id, false));
    state.detail.focused_field = 0; // Username

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let _result = state.update(Message::KeyEvent(make_key(KeyCode::Char('j'))), &mut ctx);

    // Should have moved to password field (index 1)
    assert_eq!(state.detail.focused_field, 1);
}

#[test]
fn k_on_detail_moves_field_up() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState {
        focused_panel: PanelId::Detail,
        ..Default::default()
    };
    state.detail.record = Some(make_detail_view_with_fields(id, false));
    state.detail.focused_field = 1; // Password

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let _result = state.update(Message::KeyEvent(make_key(KeyCode::Char('k'))), &mut ctx);

    // Should have moved to username field (index 0)
    assert_eq!(state.detail.focused_field, 0);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn right_on_detail_focuses_first_action_button() {
    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(id, false));

    let mut ctx = make_ctx();
    let result = state.update(Message::KeyEvent(key_event(KeyCode::Right)), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(
        state.detail.focused_action,
        Some(crate::tui::state::detail_state::DetailActionFocus {
            field_index: 0,
            kind: crate::tui::state::detail_state::DetailActionKind::Copy
        })
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn enter_on_focused_detail_action_copies_that_field() {
    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(id, false));
    state.detail.focused_action = Some(crate::tui::state::detail_state::DetailActionFocus {
        field_index: 0,
        kind: crate::tui::state::detail_state::DetailActionKind::Copy,
    });

    let mut ctx = make_ctx();
    let result = state.update(Message::KeyEvent(key_event(KeyCode::Enter)), &mut ctx);

    match result {
        ScreenResult::Command(cmd) => match &*cmd {
            Command::CopyToClipboard {
                id: cmd_id,
                field: FieldSelector::Username,
            } => assert_eq!(*cmd_id, id),
            other => panic!("Expected username copy action, got {:?}", other),
        },
        other => panic!("Expected command, got {:?}", other),
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn down_on_detail_action_moves_to_same_action_on_next_row() {
    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(id, false));
    state.detail.focused_action = Some(crate::tui::state::detail_state::DetailActionFocus {
        field_index: 0,
        kind: crate::tui::state::detail_state::DetailActionKind::Copy,
    });

    let mut ctx = make_ctx();
    let result = state.update(Message::KeyEvent(key_event(KeyCode::Down)), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(
        state.detail.focused_action,
        Some(crate::tui::state::detail_state::DetailActionFocus {
            field_index: 1,
            kind: crate::tui::state::detail_state::DetailActionKind::Copy
        })
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn right_on_detail_action_moves_horizontally_within_row() {
    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.detail.record = Some(make_detail_view_with_fields(id, false));
    state.detail.focused_action = Some(crate::tui::state::detail_state::DetailActionFocus {
        field_index: 1,
        kind: crate::tui::state::detail_state::DetailActionKind::ToggleSecret,
    });

    let mut ctx = make_ctx();
    let result = state.update(Message::KeyEvent(key_event(KeyCode::Right)), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(
        state.detail.focused_action,
        Some(crate::tui::state::detail_state::DetailActionFocus {
            field_index: 1,
            kind: crate::tui::state::detail_state::DetailActionKind::Copy
        })
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn mouse_click_detail_password_copy_action_copies_password() {
    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
    state.terminal_area = Rect::new(0, 0, 140, 30);
    state.detail.record = Some(make_detail_view_with_fields(id, false));

    let layout = crate::tui::screens::main::layout::calculate_layout(state.terminal_area, 140);
    let detail_rect = Rect::new(
        layout.detail.x,
        layout.detail.y + 1,
        layout.detail.width,
        layout.detail.height.saturating_sub(1),
    );
    let column = detail_rect.right().saturating_sub(8);
    let row = detail_rect.y + 12;

    let mut ctx = make_ctx();
    let result = state.update(Message::MouseEvent(mouse_click(column, row)), &mut ctx);

    match result {
        ScreenResult::Command(cmd) => match &*cmd {
            Command::CopyToClipboard {
                id: cmd_id,
                field: FieldSelector::Password,
            } => assert_eq!(*cmd_id, id),
            other => panic!("Expected password copy action, got {:?}", other),
        },
        other => panic!("Expected command, got {:?}", other),
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn field_decrypted_updates_password_visibility() {
    use crate::commands::result::CommandResult;
    use crate::commands::types::FieldSelector;

    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.detail.record = Some(make_detail_view_with_fields(id, false));
    state.detail.password_visible = false;

    let value = SecureStr::new("revealed-password".to_string());

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::FieldDecrypted {
            id,
            field: FieldSelector::Password,
            value,
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert!(state.detail.password_visible);
    if let Some(ref record) = state.detail.record {
        let password_field = &record.fields[1]; // Password at index 1
        match &password_field.value {
            crate::tui::state::detail_state::FieldValue::Revealed(s) => {
                assert_eq!(s, "revealed-password");
            }
            other => panic!("Expected Revealed, got {:?}", other),
        }
    } else {
        panic!("Expected detail record");
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn password_history_loaded_opens_overlay() {
    use crate::commands::result::CommandResult;
    use crate::types::PasswordHistoryView;

    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.detail.record = Some(make_detail_view_with_fields(id, false));

    let history = vec![PasswordHistoryView {
        id: 1,
        password: SecureStr::new("old-password".to_string()),
        changed_at: chrono::Utc::now(),
    }];

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let result = state.update(
        Message::CommandCompleted(CommandResult::PasswordHistoryLoaded { history }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    // Verify overlay is active and is PasswordHistory
    assert!(state.overlay_manager.is_active());
    match state.overlay_manager.get() {
        Some(crate::tui::screens::main::overlay::ActiveOverlay::PasswordHistory(phs)) => {
            assert_eq!(phs.entries.len(), 1);
            assert_eq!(phs.record_name, "Test Detail");
        }
        other => panic!("Expected PasswordHistory overlay, got {:?}", other),
    }
}

#[test]
fn batch_tag_add_keeps_overlay_active_and_visual_selection() {
    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.list = ListPanelState::with_records(vec![make_test_record_with_name(id, "GitHub")]);
    state.list.enter_visual();
    state.list.select_all();
    state.overlay_manager.open(Overlay::BatchTagPanel(
        crate::commands::types::BatchTagPanelState {
            record_ids: vec![id],
            selected_record_names: vec!["GitHub".to_string()],
            current_tag: String::new(),
            current_tags: Vec::new(),
            available_tags: vec!["work".to_string(), "personal".to_string()],
        },
    ));

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        &mut ctx,
    );
    assert!(matches!(result, ScreenResult::Continue));
    let result = state.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        &mut ctx,
    );
    assert!(matches!(result, ScreenResult::Continue));
    let result = state.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        &mut ctx,
    );

    match result {
        ScreenResult::Command(command) => match *command {
            Command::BatchAddTag {
                record_ids,
                tag_name,
            } => {
                assert_eq!(record_ids, vec![id]);
                assert_eq!(tag_name, "work");
            }
            other => panic!("Expected BatchAddTag command, got {other:?}"),
        },
        other => panic!("Expected command result, got {other:?}"),
    }
    assert!(state.overlay_manager.is_active());
    assert!(state.list.is_visual());
}

// ── Task 8: Help overlay toggle test ─────────────────────────────────────────

#[test]
fn question_mark_opens_help_overlay() {
    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let mut state = MainScreenState::default();

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let _result = state.update(Message::KeyEvent(make_key(KeyCode::Char('?'))), &mut ctx);

    assert!(state.overlay_manager.is_active());
    match state.overlay_manager.get() {
        Some(crate::tui::screens::main::overlay::ActiveOverlay::Help) => {}
        other => panic!("Expected Help overlay, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// VaultLocked cleanup tests
// ---------------------------------------------------------------------------

#[test]
fn vault_locked_clears_state_and_navigates_to_unlock() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.list.records = vec![make_test_record(Some(id1)), make_test_record(Some(id2))];
    state.list.selected_index = Some(1);
    state.overlay_manager.open(Overlay::Help);

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(
        Message::CommandCompleted(crate::commands::result::CommandResult::VaultLocked),
        &mut ctx,
    );

    assert!(state.list.records.is_empty());
    assert!(state.list.selected_index.is_none());
    assert!(state.detail.record.is_none());
    assert!(!state.overlay_manager.is_active());
    assert!(matches!(
        result,
        ScreenResult::NavigateTo(crate::commands::types::Screen::Unlock)
    ));
}

// ---------------------------------------------------------------------------
// Search detail loading tests
// ---------------------------------------------------------------------------

#[test]
fn search_typing_loads_detail_for_selected_record() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::List;
    state.list.records = vec![
        make_test_record_with_name(id1, "Alpha"),
        make_test_record_with_name(id2, "Beta"),
    ];
    state.list.selected_index = Some(0);
    state.list.enter_search();

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    // Type 'a' — should filter to "Alpha" and emit LoadRecordDetail
    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let result = state.update(Message::KeyEvent(key), &mut ctx);

    match result {
        ScreenResult::Command(cmd) => {
            let expected = Box::new(Command::LoadRecordDetail { id: id1 });
            assert_eq!(
                format!("{:?}", cmd),
                format!("{:?}", expected),
                "Expected LoadRecordDetail for filtered record"
            );
        }
        other => panic!("Expected Command(LoadRecordDetail), got {:?}", other),
    }
}

#[test]
fn search_down_arrow_loads_detail() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::List;
    state.list.records = vec![
        make_test_record_with_name(id1, "Alpha"),
        make_test_record_with_name(id2, "Beta"),
    ];
    state.list.selected_index = Some(0);
    state.list.enter_search();

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
    let result = state.update(Message::KeyEvent(key), &mut ctx);

    match result {
        ScreenResult::Command(cmd) => {
            let expected = Box::new(Command::LoadRecordDetail { id: id2 });
            assert_eq!(
                format!("{:?}", cmd),
                format!("{:?}", expected),
                "Expected LoadRecordDetail for second record after Down"
            );
        }
        other => panic!("Expected Command(LoadRecordDetail), got {:?}", other),
    }
}

#[test]
fn search_esc_restores_and_loads_detail() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::List;
    state.list.records = vec![
        make_test_record_with_name(id1, "Alpha"),
        make_test_record_with_name(id2, "Bravo"),
    ];
    state.list.selected_index = Some(1);
    state.list.enter_search();

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let result = state.update(Message::KeyEvent(key), &mut ctx);

    // Should restore pre-search selection (index 1, id2) and load its detail
    assert_eq!(state.list.selected_index, Some(1));
    match result {
        ScreenResult::Command(cmd) => {
            let expected = Box::new(Command::LoadRecordDetail { id: id2 });
            assert_eq!(
                format!("{:?}", cmd),
                format!("{:?}", expected),
                "Expected LoadRecordDetail for restored selection after Esc"
            );
        }
        other => panic!("Expected Command(LoadRecordDetail), got {:?}", other),
    }
}

fn make_test_record_with_name(id: Uuid, name: &str) -> TuiRecord {
    TuiRecord {
        id,
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

#[test]
fn vault_locked_clears_search_snapshot() {
    let id1 = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::List;
    state.list.records = vec![make_test_record_with_name(id1, "Alpha")];
    state.list.selected_index = Some(0);
    // Enter search to create a snapshot
    state.list.enter_search();

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    let result = state.update(
        Message::CommandCompleted(crate::commands::result::CommandResult::VaultLocked),
        &mut ctx,
    );

    assert!(matches!(
        state.list.mode,
        crate::tui::state::list_state::ListMode::Normal
    ));
    assert!(state.list.records.is_empty());
    assert!(state.list.selected_index.is_none());
    assert!(state.detail.record.is_none());
    assert!(matches!(result, ScreenResult::NavigateTo(_)));
}

#[test]
fn search_no_results_clears_detail() {
    let id1 = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::List;
    state.list.records = vec![make_test_record_with_name(id1, "Alpha")];
    state.list.selected_index = Some(0);
    state.list.enter_search();

    // Simulate having a detail loaded by setting a non-None password_visible
    // (detail.clear() sets password_visible = false and record = None)
    state.detail.password_visible = true;

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    // Type 'z' — no match, should clear detail
    let key = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);
    let _result = state.update(Message::KeyEvent(key), &mut ctx);

    assert!(
        state.list.records.is_empty(),
        "Filter should produce no results"
    );
    assert!(
        !state.detail.password_visible,
        "Detail should be cleared when no records match"
    );
}

// ── Issue #123: trash navigation → detail → restore/hard-delete ──────────

#[test]
fn trash_navigation_then_restore_full_flow() {
    use crate::commands::result::CommandResult;
    use crate::commands::types::RecordFilter;
    use crate::types::credential::CredentialType;
    use crate::types::record::{DecryptedRecord, TuiRecord};
    use crate::types::SecureStr;

    let id0 = Uuid::new_v4();
    let id1 = Uuid::new_v4();
    let records = vec![
        TuiRecord {
            id: id0,
            credential_type: CredentialType::Login,
            name: "Deleted-A".to_string(),
            subtitle: String::new(),
            is_favorite: false,
            is_expired: false,
            expires_at: None,
            has_weak_password: false,
            is_compromised: false,
            duplicate_group_size: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted: true,
            deleted_at: Some(chrono::Utc::now()),
            tags: Vec::new(),
            sync_status: None,
        },
        TuiRecord {
            id: id1,
            credential_type: CredentialType::Login,
            name: "Deleted-B".to_string(),
            subtitle: String::new(),
            is_favorite: false,
            is_expired: false,
            expires_at: None,
            has_weak_password: false,
            is_compromised: false,
            duplicate_group_size: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted: true,
            deleted_at: Some(chrono::Utc::now()),
            tags: Vec::new(),
            sync_status: None,
        },
    ];

    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::List;
    state.current_filter = RecordFilter::Trash;
    state.list.records = records;
    state.list.selected_index = Some(0);

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    // Step 1: press j to navigate to second record
    let result = state.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
        &mut ctx,
    );
    assert_eq!(state.list.selected_index, Some(1));
    let selected_id = match &result {
        ScreenResult::Command(cmd) => match cmd.as_ref() {
            Command::LoadRecordDetail { id } => *id,
            _ => panic!("expected LoadRecordDetail, got {:?}", cmd),
        },
        _ => panic!("expected Command result from trash j, got {:?}", result),
    };
    assert_eq!(selected_id, id1, "j should select second record");

    // Step 2: apply RecordDetailLoaded for the navigated record
    let decrypted = DecryptedRecord::Login {
        id: selected_id,
        is_favorite: false,
        expires_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        version: 1,
        deleted: true,
        deleted_at: Some(chrono::Utc::now()),
        tags: Vec::new(),
        name: "Deleted-B".to_string(),
        username: "user".to_string(),
        password: SecureStr::new("pass".to_string()),
        url: None,
        notes: None,
    };
    state.update(
        Message::CommandCompleted(CommandResult::RecordDetailLoaded {
            record: decrypted,
            password_strength: None,
            health_issue: None,
        }),
        &mut ctx,
    );

    // Step 3: assert detail loaded with correct id and is_trash
    assert!(
        state.detail.record.is_some(),
        "detail should have a record loaded"
    );
    assert_eq!(state.detail.record.as_ref().unwrap().id, selected_id);
    assert!(
        state.detail.is_trash,
        "detail should be marked as trash context"
    );

    // Step 4: press r to open restore confirm
    let result = state.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
        &mut ctx,
    );
    // Overlay should be opened (state.update routes through MainScreen::handle_key_event)
    assert!(
        state.overlay_manager.is_active(),
        "restore confirm overlay should be open"
    );
    drop(result);

    // Step 5: Restore defaults to Confirm, press Enter directly
    let result = state.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        &mut ctx,
    );
    assert!(
        !state.overlay_manager.is_active(),
        "overlay should close after confirm"
    );
    match result {
        ScreenResult::Command(cmd) => match cmd.as_ref() {
            Command::RestoreRecord { id } => {
                assert_eq!(*id, selected_id, "should restore the navigated record");
            }
            _ => panic!("expected RestoreRecord, got {:?}", cmd),
        },
        _ => panic!("expected Command from restore confirm, got {:?}", result),
    }
}

#[test]
fn trash_navigation_then_hard_delete_full_flow() {
    use crate::commands::result::CommandResult;
    use crate::commands::types::RecordFilter;
    use crate::types::credential::CredentialType;
    use crate::types::record::{DecryptedRecord, TuiRecord};
    use crate::types::SecureStr;

    let id0 = Uuid::new_v4();
    let id1 = Uuid::new_v4();
    let records = vec![
        TuiRecord {
            id: id0,
            credential_type: CredentialType::Login,
            name: "Deleted-A".to_string(),
            subtitle: String::new(),
            is_favorite: false,
            is_expired: false,
            expires_at: None,
            has_weak_password: false,
            is_compromised: false,
            duplicate_group_size: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted: true,
            deleted_at: Some(chrono::Utc::now()),
            tags: Vec::new(),
            sync_status: None,
        },
        TuiRecord {
            id: id1,
            credential_type: CredentialType::Login,
            name: "Deleted-B".to_string(),
            subtitle: String::new(),
            is_favorite: false,
            is_expired: false,
            expires_at: None,
            has_weak_password: false,
            is_compromised: false,
            duplicate_group_size: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted: true,
            deleted_at: Some(chrono::Utc::now()),
            tags: Vec::new(),
            sync_status: None,
        },
    ];

    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::List;
    state.current_filter = RecordFilter::Trash;
    state.list.records = records;
    state.list.selected_index = Some(0);

    let (tx, _rx) = mpsc::channel(16);
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &Default::default(),
    };

    // Navigate to second record
    let result = state.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
        &mut ctx,
    );
    let selected_id = match &result {
        ScreenResult::Command(cmd) => match cmd.as_ref() {
            Command::LoadRecordDetail { id } => *id,
            _ => panic!("expected LoadRecordDetail"),
        },
        _ => panic!("expected Command from trash j"),
    };

    // Load detail
    let decrypted = DecryptedRecord::Login {
        id: selected_id,
        is_favorite: false,
        expires_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        version: 1,
        deleted: true,
        deleted_at: Some(chrono::Utc::now()),
        tags: Vec::new(),
        name: "Deleted-B".to_string(),
        username: "user".to_string(),
        password: SecureStr::new("pass".to_string()),
        url: None,
        notes: None,
    };
    state.update(
        Message::CommandCompleted(CommandResult::RecordDetailLoaded {
            record: decrypted,
            password_strength: None,
            health_issue: None,
        }),
        &mut ctx,
    );
    assert!(state.detail.record.is_some());
    assert!(state.detail.is_trash);

    // Press D (Shift+D) to open hard-delete confirm
    let result = state.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::NONE)),
        &mut ctx,
    );
    assert!(
        state.overlay_manager.is_active(),
        "hard delete confirm overlay should be open"
    );
    drop(result);

    // Tab to Confirm, Enter to confirm
    state.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        &mut ctx,
    );
    let result = state.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        &mut ctx,
    );
    match result {
        ScreenResult::Command(cmd) => match cmd.as_ref() {
            Command::HardDeleteRecord { id } => {
                assert_eq!(*id, selected_id, "should hard-delete the navigated record");
            }
            _ => panic!("expected HardDeleteRecord, got {:?}", cmd),
        },
        _ => panic!(
            "expected Command from hard-delete confirm, got {:?}",
            result
        ),
    }
}
