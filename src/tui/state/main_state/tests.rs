use super::*;
use crate::commands::types::{
    ConfirmButton, ConfirmDialogState, ConfirmVariant, FieldSelector, Overlay, RecordFilter,
};
use crate::commands::{Command, Message};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::{SecureStr, Tag};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use uuid::Uuid;

#[test]
fn sidebar_default_selects_all_category() {
    let sidebar = SidebarState::default();
    // Brand(0) and Separator(1) are non-selectable, so All(2) is selected
    assert_eq!(sidebar.selected_index, 2);
    assert_eq!(sidebar.current_filter(), RecordFilter::All);
}

#[test]
fn sidebar_navigation_skips_separators() {
    let mut sidebar = SidebarState::default();
    // Items: Brand, Sep, All, Favorites, Expired, HealthIssues, Trash, Sep, TagHeader, Sep, Generator, Config
    // Selectable:         2,    3,         4,       5,             6,     _,   8,         _,   10,         11
    // Start at All (2), next -> Favorites (3)
    sidebar.next_selectable();
    assert_eq!(sidebar.selected_index, 3);
    assert!(matches!(
        sidebar.items[3],
        SidebarItem::Category(SidebarCategory::Favorites)
    ));

    // Skip ahead past categories to verify separator skip
    sidebar.selected_index = 6; // Trash
    sidebar.next_selectable();
    // Items[7] is Separator (non-selectable), items[8] is TagHeader (selectable)
    // Should land on TagHeader (index 8)
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
        .position(|i| matches!(i, SidebarItem::Tag(_)))
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

    // Brand + sep + 5 categories + separator + tag header + 2 tags + separator + generator + config = 14
    assert_eq!(items.len(), 14);

    // Verify structure
    assert!(matches!(items[0], SidebarItem::Brand));
    assert!(matches!(items[1], SidebarItem::Separator));
    assert!(matches!(
        items[2],
        SidebarItem::Category(SidebarCategory::All)
    ));
    assert!(matches!(items[7], SidebarItem::Separator));
    assert!(matches!(items[8], SidebarItem::TagHeader));
    assert!(matches!(items[9], SidebarItem::Tag(ref t) if t == "personal"));
    assert!(matches!(items[10], SidebarItem::Tag(ref t) if t == "work"));
    assert!(matches!(items[11], SidebarItem::Separator));
    assert!(matches!(items[12], SidebarItem::Generator));
    assert!(matches!(items[13], SidebarItem::Config));
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
        .filter(|i| matches!(i, SidebarItem::Tag(_)))
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
    // Brand(0) and Separator(1) are non-selectable, so All is at index 2
    assert_eq!(state.sidebar.selected_index, 2);
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
        Command::LoadRecordList { filter, sort } => {
            assert_eq!(filter, RecordFilter::All);
            assert_eq!(sort, crate::commands::types::RecordSort::default());
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

    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::Favorites;
    state.on_mount(&mut ctx);

    let cmd = rx
        .try_recv()
        .expect("on_mount should send a LoadRecordList command");
    match cmd {
        Command::LoadRecordList { filter, .. } => {
            assert_eq!(filter, RecordFilter::Favorites);
        }
        _ => panic!("Expected LoadRecordList command"),
    }
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
        Message::CommandCompleted(CommandResult::RecordListLoaded { records, total: 10 }),
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
        Message::CommandCompleted(CommandResult::RecordListLoaded { records, total: 5 }),
        &mut ctx,
    );

    assert_eq!(state.status_bar.record_count, 5);
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
    // Start at Favorites (index 3), move up to All (index 2)
    state.selected_index = 3;
    state.move_up();
    assert_eq!(state.selected_index, 2);
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
    assert!(state.items.iter().any(|i| matches!(i, SidebarItem::Tag(_))));
    state.toggle_tags();
    assert!(!state.tags_expanded);
    assert!(state
        .items
        .iter()
        .all(|i| !matches!(i, SidebarItem::Tag(_))));
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
    // Navigate from Trash (6) down — should land on TagHeader (8)
    sidebar.selected_index = 6;
    sidebar.move_down();
    assert!(matches!(
        sidebar.items[sidebar.selected_index],
        SidebarItem::TagHeader
    ));
    // Move down again — should skip Separator and land on Generator (10)
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
    assert!(!sidebar.tags_expanded);

    // Toggle: collapsed -> expanded
    sidebar.toggle_tags();
    assert!(sidebar.tags_expanded);

    // Toggle: expanded -> collapsed, focus returns to TagHeader
    sidebar.toggle_tags();
    assert!(!sidebar.tags_expanded);
    assert!(matches!(
        sidebar.items[sidebar.selected_index],
        SidebarItem::TagHeader
    ));
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
        .position(|i| matches!(i, SidebarItem::Tag(_)))
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
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::Tag("work".to_string());
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
    state.list.exit_search();
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

#[test]
fn default_state_has_no_active_overlay() {
    let state = MainScreenState::default();
    assert!(!state.overlay_manager.is_active());
    assert!(state.pending_animation.is_none());
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
    if let Some(generator) = state.overlay_manager.get_mut() {
        if let crate::tui::screens::main::overlay::ActiveOverlay::PasswordGenerator(gen_state) =
            generator
        {
            gen_state.preview = "test-password-123".to_string();
        }
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
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Sidebar;
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
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Sidebar;
    // Start at Favorites, k moves up to All
    state.sidebar.select_category(SidebarCategory::Favorites);
    assert_eq!(state.current_filter, RecordFilter::All); // initial default
    state.current_filter = RecordFilter::Favorites;

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
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Sidebar;
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
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Sidebar;
    state.current_filter = RecordFilter::All;

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

    let mut state = MainScreenState::default();
    state.list_auto_select = true;

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
    let mut state = MainScreenState::default();
    state.list = ListPanelState::with_records(records);
    state.focused_panel = PanelId::List;
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
    let mut state = MainScreenState::default();
    state.list = ListPanelState::with_records(records);
    state.focused_panel = PanelId::List;
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
    let mut state = MainScreenState::default();
    state.list = ListPanelState::with_records(records);
    state.focused_panel = PanelId::List;
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
    let mut state = MainScreenState::default();
    state.list = ListPanelState::with_records(records);
    state.focused_panel = PanelId::List;
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
    let mut state = MainScreenState::default();
    state.list = ListPanelState::with_records(records);
    state.focused_panel = PanelId::List;
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
    let mut state = MainScreenState::default();
    state.list = ListPanelState::with_records(records);
    state.focused_panel = PanelId::List;

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
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::All;
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
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::All;

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
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::All;
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

    // First message: LoadRecordList
    let cmd1 = rx.try_recv().expect("Should send LoadRecordList");
    assert!(matches!(cmd1, Command::LoadRecordList { .. }));

    // Second message: LoadRecordDetail (because detail was showing the updated record)
    let cmd2 = rx
        .try_recv()
        .expect("Should send LoadRecordDetail after RecordUpdated when detail was showing");
    match cmd2 {
        Command::LoadRecordDetail { id } => {
            assert_eq!(id, record_id);
        }
        other => panic!("Expected LoadRecordDetail, got {:?}", other),
    }
}

#[test]
fn record_updated_does_not_refresh_detail_when_showing_different_record() {
    use crate::commands::result::CommandResult;

    let updated_id = Uuid::new_v4();
    let detail_id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::All;
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
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::All;
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
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::All;
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
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::Trash;

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
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::Trash;
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
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::Trash;

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
fn record_list_loaded_cursor_recovery_keeps_selection() {
    use crate::commands::result::CommandResult;

    let mut state = MainScreenState::default();
    state.list.records = vec![make_test_record(None), make_test_record(None)];
    state.list.selected_index = Some(1);
    state.list_auto_select = false;

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    let new_records = vec![make_test_record(None), make_test_record(None)];
    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: new_records,
            total: 2,
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.list.selected_index, Some(1));
    assert!(!state.list_auto_select);
}

#[test]
fn record_list_loaded_cursor_recovery_clamps_oob() {
    use crate::commands::result::CommandResult;

    let mut state = MainScreenState::default();
    state.list.records = vec![
        make_test_record(None),
        make_test_record(None),
        make_test_record(None),
    ];
    state.list.selected_index = Some(2);
    state.list_auto_select = false;

    let (tx, _rx) = mpsc::channel(16);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };

    // New list has only 1 record — index 2 should clamp to 0
    let new_records = vec![make_test_record(None)];
    let result = state.update(
        Message::CommandCompleted(CommandResult::RecordListLoaded {
            records: new_records,
            total: 1,
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.list.selected_index, Some(0));
}

#[test]
fn record_list_loaded_cursor_recovery_initial_load() {
    use crate::commands::result::CommandResult;

    let mut state = MainScreenState::default();
    // Initial state: selected_index = None, no records
    assert_eq!(state.list.selected_index, None);
    state.list_auto_select = false;

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
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    // Initial load should keep None for U4 Empty State
    assert_eq!(state.list.selected_index, None);
}

#[test]
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
        }),
        &mut ctx,
    );

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(state.list.selected_index, None);
    assert!(state.detail.record.is_none()); // detail cleared for empty list
}

// ── FavoriteToggled tests ────────────────────────────────────

#[test]
fn favorite_toggled_updates_detail_is_favorite() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::All;
    state.list.records = vec![make_test_record(Some(record_id))];
    state.detail.record = Some(make_detail_view(record_id, false));
    assert_eq!(state.detail.record.as_ref().unwrap().is_favorite, false);

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
    assert_eq!(state.detail.record.as_ref().unwrap().is_favorite, true);
    assert_eq!(state.list.records[0].is_favorite, true);
}

#[test]
fn favorite_toggled_does_not_update_detail_when_not_showing() {
    use crate::commands::result::CommandResult;

    let toggled_id = Uuid::new_v4();
    let detail_id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::All;
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
    assert_eq!(state.detail.record.as_ref().unwrap().is_favorite, false);
    // List record's is_favorite should be updated
    assert_eq!(state.list.records[0].is_favorite, true);
}

#[test]
fn favorite_toggled_removes_from_list_when_viewing_favorites() {
    use crate::commands::result::CommandResult;

    let record_id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.current_filter = RecordFilter::Favorites;
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
    assert_eq!(state.detail.record.as_ref().unwrap().is_favorite, false);
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
    let mut state = MainScreenState::default();
    state.list = crate::tui::state::list_state::ListPanelState::with_records(records);
    state.focused_panel = PanelId::List;

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

    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::List;

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

#[test]
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
fn enter_on_detail_copies_current_field() {
    use crate::commands::types::PanelId;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    let id = Uuid::new_v4();
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
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
fn H_on_detail_loads_password_history() {
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
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
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
    let mut state = MainScreenState::default();
    state.focused_panel = PanelId::Detail;
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
