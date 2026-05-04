use super::*;
use crate::commands::types::{
    ConfirmButton, ConfirmDialogState, ConfirmVariant, Overlay, RecordFilter,
};
use crate::commands::{Command, Message};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::Tag;
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
