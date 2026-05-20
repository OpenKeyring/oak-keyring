use chrono::{TimeZone, Utc};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use uuid::Uuid;

use oak_keyring::commands::types::{
    BatchTagPanelState, ConfirmButton, ConfirmDialogState, ConfirmVariant, ErrorDialogState,
    Overlay, PanelId, RecordFilter,
};
use oak_keyring::crypto::strength::evaluate_strength;
use oak_keyring::tui::screens::main::overlay::ActiveOverlay;
use oak_keyring::tui::screens::main::MainScreen;
use oak_keyring::tui::state::detail_state::{
    DetailField, DetailFieldKind, DetailPanelState, DetailViewData, ExpiryStatus, FieldValue,
};
use oak_keyring::tui::state::generator_state::GeneratorFocus;
use oak_keyring::tui::state::list_state::ListPanelState;
use oak_keyring::tui::state::main_state::{
    CategoryCounts, MainScreenState, SidebarCategory, SidebarItem,
};
use oak_keyring::tui::state::overlay_state::HistoryEntry;
use oak_keyring::tui::state::tag_management::InlineEditState;
use oak_keyring::types::credential::CredentialType;
use oak_keyring::types::record::TuiRecord;
use oak_keyring::types::sensitive::SensitiveInput;
use oak_keyring::types::Tag;

use crate::support::snapshot_locale;

fn fixed_past_ts() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap()
}

fn make_record(id: Uuid, name: &str, subtitle: &str) -> TuiRecord {
    TuiRecord {
        id,
        credential_type: CredentialType::Login,
        name: name.to_string(),
        subtitle: subtitle.to_string(),
        is_favorite: false,
        is_expired: false,
        expires_at: None,
        has_weak_password: false,
        is_compromised: false,
        duplicate_group_size: None,
        created_at: fixed_past_ts(),
        updated_at: fixed_past_ts(),
        deleted: false,
        deleted_at: None,
        tags: Vec::new(),
        sync_status: None,
    }
}

fn sensitive(s: &str) -> SensitiveInput {
    let mut input = SensitiveInput::new();
    for c in s.chars() {
        input.push_char(c);
    }
    input
}

fn make_trash_record(id: Uuid, name: &str, days_ago: i64) -> TuiRecord {
    let deleted_at = Utc::now() - chrono::Duration::try_days(days_ago).unwrap();
    TuiRecord {
        deleted: true,
        deleted_at: Some(deleted_at),
        updated_at: deleted_at,
        ..make_record(id, name, "")
    }
}

fn populate_state(state: &mut MainScreenState) {
    let rec1_id = Uuid::nil();
    let rec2_id = Uuid::new_v4();

    // 1. Sidebar state
    state.sidebar.category_counts = CategoryCounts {
        all: 2,
        favorites: 1,
        expired: 0,
        health_issues: 0,
        trash: 0,
    };
    state.sidebar.items = vec![
        SidebarItem::Category(SidebarCategory::All),
        SidebarItem::Category(SidebarCategory::Favorites),
        SidebarItem::Category(SidebarCategory::Expired),
    ];
    state.sidebar.selected_index = 0;

    // 2. List state
    let rec1 = make_record(rec1_id, "GitHub Account", "octocat");
    let rec2 = make_record(rec2_id, "Personal Email", "alice@example.com");
    state.list = ListPanelState::default();
    state.list.records = vec![rec1, rec2];
    state.list.selected_index = Some(0);

    // 3. Detail state
    let detail_data = DetailViewData {
        id: rec1_id,
        name: "GitHub Account".to_string(),
        subtitle: "octocat".to_string(),
        credential_type: CredentialType::Login,
        is_favorite: true,
        expires_at: None,
        expiry_status: ExpiryStatus::None,
        tags: vec!["work".to_string(), "personal".to_string()],
        notes: Some("Development credentials".to_string()),
        created_at: fixed_past_ts(),
        updated_at: fixed_past_ts(),
        fields: vec![
            DetailField {
                label: "Username".to_string(),
                value: FieldValue::Plain("octocat".to_string()),
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
    };
    state.detail = DetailPanelState::with_record(detail_data);
}

fn open_overlay(state: &mut MainScreenState, overlay: Overlay) {
    assert!(state.overlay_manager.open(overlay));
}

fn render_screen(
    state: &MainScreenState,
    focused_panel: PanelId,
    width: u16,
    height: u16,
) -> TestBackend {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let main_screen = MainScreen::new();
    terminal
        .draw(|frame| {
            main_screen.view(frame, frame.area(), state, focused_panel, true);
        })
        .unwrap();
    terminal.backend().clone()
}

#[test]
fn main_composition_sidebar_focused() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    let backend = render_screen(&state, PanelId::Sidebar, 100, 24);
    insta::assert_snapshot!("main_composition_sidebar_focused", backend);
}

#[test]
fn main_composition_list_focused() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    let backend = render_screen(&state, PanelId::List, 100, 24);
    insta::assert_snapshot!("main_composition_list_focused", backend);
}

#[test]
fn main_composition_detail_focused() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    let backend = render_screen(&state, PanelId::Detail, 100, 24);
    insta::assert_snapshot!("main_composition_detail_focused", backend);
}

#[test]
fn main_composition_search_no_results() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    state.list.enter_search();
    state.list.update_search_query("missing".to_string());
    state.list.records.clear();
    state.list.selected_index = None;
    state.detail = DetailPanelState::default();
    let backend = render_screen(&state, PanelId::List, 100, 24);
    insta::assert_snapshot!("main_composition_search_no_results", backend);
}

#[test]
fn main_composition_visual_selection() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    state.list.enter_visual();
    state.list.toggle_select_current();
    state.list.selected_index = Some(0);
    let backend = render_screen(&state, PanelId::List, 100, 24);
    insta::assert_snapshot!("main_composition_visual_selection", backend);
}

#[test]
fn main_composition_trash_view() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    state.current_filter = RecordFilter::Trash;
    state.sidebar.select_category(SidebarCategory::Trash);
    state.sidebar.category_counts.trash = 2;
    state.list = ListPanelState::with_records(vec![
        make_trash_record(Uuid::nil(), "Old GitHub Login", 5),
        make_trash_record(Uuid::from_u128(2), "Expired API Key", 20),
    ]);
    state.detail = DetailPanelState::default();
    let backend = render_screen(&state, PanelId::List, 100, 24);
    insta::assert_snapshot!("main_composition_trash_view", backend);
}

#[test]
fn main_composition_sidebar_inline_rename() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    state.sidebar.tags_expanded = true;
    state.sidebar.tag_management_mode = true;
    state.sidebar.tags = vec![
        Tag {
            id: 1,
            name: "work".to_string(),
        },
        Tag {
            id: 2,
            name: "personal".to_string(),
        },
    ];
    state.sidebar.rebuild();
    state.sidebar.selected_index = state
        .sidebar
        .items
        .iter()
        .position(|item| matches!(item, SidebarItem::Tag(name, _) if name == "work"))
        .unwrap();
    state.sidebar.tag_management.inline_edit = Some(InlineEditState {
        original_name: "work".to_string(),
        text: "work-critical".to_string(),
        cursor: "work-critical".len(),
        conflict: false,
    });
    let backend = render_screen(&state, PanelId::Sidebar, 100, 24);
    insta::assert_snapshot!("main_composition_sidebar_inline_rename", backend);
}

#[test]
fn main_composition_help_overlay() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    open_overlay(&mut state, Overlay::Help);
    let backend = render_screen(&state, PanelId::List, 100, 24);
    insta::assert_snapshot!("main_composition_help_overlay", backend);
}

#[test]
fn main_composition_generator_overlay() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    open_overlay(&mut state, Overlay::PasswordGenerator);
    if let Some(ActiveOverlay::PasswordGenerator(generator)) = state.overlay_manager.get_mut() {
        generator.preview = sensitive("CorrectHorse42!");
        generator.strength = Some(evaluate_strength("CorrectHorse42!"));
        generator.focus = GeneratorFocus::ActionButton;
    }
    let backend = render_screen(&state, PanelId::Sidebar, 100, 24);
    insta::assert_snapshot!("main_composition_generator_overlay", backend);
}

#[test]
fn main_composition_confirm_overlay() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    open_overlay(
        &mut state,
        Overlay::ConfirmDialog(ConfirmDialogState {
            variant: ConfirmVariant::SoftDelete {
                record_id: Uuid::nil(),
                record_name: "GitHub Account".to_string(),
                auto_delete_days: Some(30),
            },
            focused_button: ConfirmButton::Cancel,
        }),
    );
    let backend = render_screen(&state, PanelId::List, 100, 24);
    insta::assert_snapshot!("main_composition_confirm_overlay", backend);
}

#[test]
fn main_composition_batch_tag_overlay() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    open_overlay(
        &mut state,
        Overlay::BatchTagPanel(BatchTagPanelState {
            record_ids: vec![Uuid::nil(), Uuid::from_u128(2)],
            current_tag: "work".to_string(),
        }),
    );
    let backend = render_screen(&state, PanelId::List, 100, 24);
    insta::assert_snapshot!("main_composition_batch_tag_overlay", backend);
}

#[test]
fn main_composition_password_history_overlay() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    open_overlay(
        &mut state,
        Overlay::PasswordHistory {
            record_id: Uuid::nil(),
        },
    );
    if let Some(ActiveOverlay::PasswordHistory(history)) = state.overlay_manager.get_mut() {
        history.record_name = "GitHub Account".to_string();
        history.entries = vec![
            HistoryEntry {
                id: 1,
                changed_at: fixed_past_ts(),
                description: "Initial password".to_string(),
            },
            HistoryEntry {
                id: 2,
                changed_at: fixed_past_ts(),
                description: "Rotated after compromise alert".to_string(),
            },
        ];
        history.selected_index = 1;
    }
    let backend = render_screen(&state, PanelId::Detail, 100, 24);
    insta::assert_snapshot!("main_composition_password_history_overlay", backend);
}

#[test]
fn main_composition_error_dialog_overlay() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    open_overlay(
        &mut state,
        Overlay::ErrorDialog(ErrorDialogState {
            title: "Sync failed".to_string(),
            message: "Unable to upload the latest vault snapshot.".to_string(),
            detail: Some("HTTP 409 conflict after retry".to_string()),
        }),
    );
    let backend = render_screen(&state, PanelId::List, 100, 24);
    insta::assert_snapshot!("main_composition_error_dialog_overlay", backend);
}

#[test]
fn main_composition_ascii_layout() {
    let _locale = snapshot_locale();
    let mut state = MainScreenState::default();
    populate_state(&mut state);
    let backend = render_screen(&state, PanelId::List, 100, 24);
    let ascii_backend = {
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let main_screen = MainScreen::new();
        terminal
            .draw(|frame| {
                main_screen.view(frame, frame.area(), &state, PanelId::List, false);
            })
            .unwrap();
        terminal.backend().clone()
    };
    insta::assert_snapshot!("main_composition_unicode_layout", backend);
    insta::assert_snapshot!("main_composition_ascii_layout", ascii_backend);
}
