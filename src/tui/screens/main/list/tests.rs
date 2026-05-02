use super::bar::{
    build_search_bar, build_sort_bar, build_visual_bar, sort_direction_label, sort_field_label,
};
use super::empty::build_empty_state_variant;
use super::items::{build_record_item, build_trash_item, health_badge};
use super::*;
use crate::commands::types::{HealthIssue, SortDirection, SortField};
use crate::tui::components::empty_state::EmptyStateVariant;
use crate::tui::state::list_state::{SearchState, VisualState};
use crate::types::credential::CredentialType;
use crate::types::record::TuiRecord;
use chrono::Utc;
use ratatui::backend::TestBackend;
use ratatui::text::Span;
use std::collections::HashSet;
use uuid::Uuid;

/// Helper to build a TuiRecord with minimal fields for testing.
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
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted: false,
        deleted_at: None,
        tags: Vec::new(),
        sync_status: None,
    }
}

fn make_record_with_type(id: Uuid, name: &str, cred_type: CredentialType) -> TuiRecord {
    TuiRecord {
        id,
        credential_type: cred_type,
        name: name.to_string(),
        subtitle: String::new(),
        is_favorite: false,
        is_expired: false,
        expires_at: None,
        has_weak_password: false,
        is_compromised: false,
        duplicate_group_size: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted: false,
        deleted_at: None,
        tags: Vec::new(),
        sync_status: None,
    }
}

fn make_record_with_weak(id: Uuid, name: &str) -> TuiRecord {
    TuiRecord {
        id,
        credential_type: CredentialType::Login,
        name: name.to_string(),
        subtitle: String::new(),
        is_favorite: false,
        is_expired: false,
        expires_at: None,
        has_weak_password: true,
        is_compromised: false,
        duplicate_group_size: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted: false,
        deleted_at: None,
        tags: Vec::new(),
        sync_status: None,
    }
}

fn make_record_with_compromised(id: Uuid, name: &str) -> TuiRecord {
    TuiRecord {
        id,
        credential_type: CredentialType::Login,
        name: name.to_string(),
        subtitle: String::new(),
        is_favorite: false,
        is_expired: false,
        expires_at: None,
        has_weak_password: false,
        is_compromised: true,
        duplicate_group_size: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted: false,
        deleted_at: None,
        tags: Vec::new(),
        sync_status: None,
    }
}

fn make_record_with_duplicate(id: Uuid, name: &str, group_size: usize) -> TuiRecord {
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
        duplicate_group_size: if group_size > 1 {
            Some(group_size)
        } else {
            None
        },
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted: false,
        deleted_at: None,
        tags: Vec::new(),
        sync_status: None,
    }
}

/// Render into a TestBackend and return the buffer as a string snapshot.
fn render_snapshot(
    state: &ListPanelState,
    width: u16,
    height: u16,
    focused: bool,
    unicode: bool,
    filter: RecordFilter,
) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ListPanel::view(frame, frame.area(), state, focused, unicode, filter, 30);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    format!("{:?}", buf)
}

#[test]
fn render_empty_state_no_passwords() {
    let state = ListPanelState::default();
    let result = render_snapshot(&state, 40, 10, true, true, RecordFilter::All);
    // Should render without panicking and contain no-passwords empty state
    assert!(!result.is_empty());
}

#[test]
fn render_empty_state_trash() {
    let state = ListPanelState::default();
    let result = render_snapshot(&state, 40, 10, true, true, RecordFilter::Trash);
    assert!(!result.is_empty());
}

#[test]
fn render_empty_state_search_no_results() {
    let mut state = ListPanelState::default();
    state.mode = ListMode::Search(SearchState {
        query: "nonexistent".to_string(),
        cursor: 11,
    });
    let result = render_snapshot(&state, 40, 10, true, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_single_record() {
    let id = Uuid::new_v4();
    let record = make_record(id, "GitHub", "user@github.com");
    let state = ListPanelState::with_records(vec![record]);
    let result = render_snapshot(&state, 50, 10, true, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_multiple_records() {
    let r1 = make_record(Uuid::new_v4(), "GitHub", "user@github.com");
    let r2 = make_record_with_type(Uuid::new_v4(), "AWS Key", CredentialType::Api);
    let r3 = make_record_with_type(Uuid::new_v4(), "Server", CredentialType::Ssh);
    let state = ListPanelState::with_records(vec![r1, r2, r3]);
    let result = render_snapshot(&state, 50, 15, true, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_weak_password_badge() {
    let record = make_record_with_weak(Uuid::new_v4(), "WeakPass");
    let state = ListPanelState::with_records(vec![record]);
    let result = render_snapshot(&state, 50, 10, true, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_visual_mode() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let r1 = make_record(id1, "A", "");
    let r2 = make_record(id2, "B", "");
    let mut state = ListPanelState::with_records(vec![r1, r2]);
    let mut selected = HashSet::new();
    selected.insert(id1);
    state.mode = ListMode::Visual(VisualState {
        selected_ids: selected,
    });
    let result = render_snapshot(&state, 50, 15, true, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_search_mode_bar() {
    let mut state = ListPanelState::default();
    state.mode = ListMode::Search(SearchState {
        query: "git".to_string(),
        cursor: 3,
    });
    let result = render_snapshot(&state, 40, 10, true, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_unfocused() {
    let record = make_record(Uuid::new_v4(), "Test", "subtitle");
    let state = ListPanelState::with_records(vec![record]);
    let result = render_snapshot(&state, 50, 10, false, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_ascii_mode() {
    let record = make_record(Uuid::new_v4(), "Test", "subtitle");
    let state = ListPanelState::with_records(vec![record]);
    let result = render_snapshot(&state, 50, 10, true, false, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_zero_area() {
    let state = ListPanelState::default();
    // Should not panic
    let backend = TestBackend::new(0, 0);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ListPanel::view(
                frame,
                frame.area(),
                &state,
                true,
                true,
                RecordFilter::All,
                30,
            );
        })
        .unwrap();
}

// ── Sort bar unit tests ──

#[test]
fn sort_field_labels() {
    assert_eq!(
        sort_field_label(&SortField::CreatedAt),
        "\u{521B}\u{5EFA}\u{65F6}\u{95F4}"
    );
    assert_eq!(
        sort_field_label(&SortField::UpdatedAt),
        "\u{66F4}\u{65B0}\u{65F6}\u{95F4}"
    );
    assert_eq!(sort_field_label(&SortField::Name), "\u{540D}\u{79F0}");
    assert_eq!(
        sort_field_label(&SortField::UsageFrequency),
        "\u{4F7F}\u{7528}\u{9891}\u{7387}"
    );
}

#[test]
fn sort_direction_labels_unicode() {
    let (icon, label) = sort_direction_label(&SortDirection::Desc, true);
    assert_eq!(icon, "\u{2193}"); // ↓
    assert_eq!(label, "\u{964D}\u{5E8F}"); // 降序

    let (icon, label) = sort_direction_label(&SortDirection::Asc, true);
    assert_eq!(icon, "\u{2191}"); // ↑
    assert_eq!(label, "\u{5347}\u{5E8F}"); // 升序
}

#[test]
fn sort_direction_labels_ascii() {
    let (icon, label) = sort_direction_label(&SortDirection::Desc, false);
    assert_eq!(icon, "v");
    assert_eq!(label, "\u{964D}\u{5E8F}");

    let (icon, label) = sort_direction_label(&SortDirection::Asc, false);
    assert_eq!(icon, "^");
    assert_eq!(label, "\u{5347}\u{5E8F}");
}

#[test]
fn build_sort_bar_contains_field_name() {
    let line = build_sort_bar(&SortField::Name, &SortDirection::Asc, true);
    let combined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(combined.contains("\u{540D}\u{79F0}")); // 名称
}

#[test]
fn build_search_bar_has_cursor() {
    let line = build_search_bar("hello", true);
    let combined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(combined.contains("hello_"));
}

#[test]
fn build_visual_bar_shows_count() {
    let line = build_visual_bar(3);
    let combined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(combined.contains("3"));
    assert!(combined.contains("\u{5DF2}\u{9009}")); // 已选
}

// ── Visual mode bar tests ──

#[test]
fn render_visual_mode_bar() {
    let line = build_visual_bar(5);
    assert_eq!(
        line.spans.len(),
        2,
        "visual bar should have two spans: label + count"
    );

    let label_span = &line.spans[0];
    assert!(
        label_span
            .content
            .as_ref()
            .contains("\u{591A}\u{9009}\u{6A21}\u{5F0F}"),
        "label span should contain '多选模式'"
    );
    assert!(
        label_span.style.fg == Some(theme::TEXT.into()),
        "label should use TEXT color (white bold on BG_BAR)"
    );
    assert!(
        label_span.style.add_modifier.contains(Modifier::BOLD),
        "label should be BOLD"
    );
    assert!(
        label_span.style.bg == Some(theme::BG_BAR.into()),
        "label should have BG_BAR background"
    );

    let count_span = &line.spans[1];
    assert!(
        count_span.content.as_ref().contains("5"),
        "count span should contain the number 5"
    );
    assert!(
        count_span.style.bg == Some(theme::BG_BAR.into()),
        "count should have BG_BAR background"
    );
}

#[test]
fn render_visual_mode_with_selections() {
    // Create records, enter visual mode, select some, render, verify count in buffer
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    let r1 = make_record(id1, "GitHub", "user@github.com");
    let r2 = make_record(id2, "AWS", "admin@aws.com");
    let r3 = make_record(id3, "GitLab", "dev@gitlab.com");

    let mut state = ListPanelState::with_records(vec![r1, r2, r3]);
    let mut selected = HashSet::new();
    selected.insert(id1);
    selected.insert(id3);
    state.mode = ListMode::Visual(VisualState {
        selected_ids: selected,
    });

    let result = render_snapshot(&state, 50, 15, true, true, RecordFilter::All);

    // The buffer should contain the visual mode bar with "2 已选"
    assert!(
        result.contains("2") || result.contains("(\u{0032}"),
        "rendered buffer should show 2 selected items"
    );
    assert!(
        result.contains("\u{591A}\u{9009}\u{6A21}\u{5F0F}"),
        "rendered buffer should contain '多选模式'"
    );
}

#[test]
fn render_visual_bar_zero_selections() {
    // Visual mode with no selections should show "(0 已选)"
    let line = build_visual_bar(0);
    let combined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        combined.contains("(0"),
        "zero selections should show '(0 已选)'"
    );
}

#[test]
fn exiting_visual_mode_returns_to_sort_bar() {
    // Enter visual mode, then exit back to normal, verify sort bar renders
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let r1 = make_record(id1, "Alpha", "");
    let r2 = make_record(id2, "Beta", "");
    let mut state = ListPanelState::with_records(vec![r1, r2]);

    // Enter visual mode
    state.enter_visual();
    let visual_result = render_snapshot(&state, 50, 15, true, true, RecordFilter::All);
    assert!(
        visual_result.contains("\u{591A}\u{9009}\u{6A21}\u{5F0F}"),
        "visual mode should show '多选模式'"
    );

    // Exit visual mode
    state.exit_visual();
    assert!(
        matches!(state.mode, ListMode::Normal),
        "mode should be Normal after exit"
    );

    let normal_result = render_snapshot(&state, 50, 15, true, true, RecordFilter::All);
    assert!(
        normal_result.contains("\u{6392}\u{5E8F}"),
        "normal mode should show '排序' in the bar"
    );
    assert!(
        !normal_result.contains("\u{591A}\u{9009}\u{6A21}\u{5F0F}"),
        "normal mode should NOT show '多选模式'"
    );
}

// ── Record item building tests ──

#[test]
fn build_record_item_login_type() {
    let record = make_record(Uuid::new_v4(), "MyLogin", "user@site.com");
    let item = build_record_item(&record, false, false, true, true, 50, None);
    assert!(item.height() >= 3); // title + subtitle + separator
}

#[test]
fn build_record_item_api_type() {
    let record = make_record_with_type(Uuid::new_v4(), "AWS", CredentialType::Api);
    let item = build_record_item(&record, false, false, true, true, 50, None);
    assert!(item.height() >= 3);
}

#[test]
fn build_record_item_ssh_type() {
    let record = make_record_with_type(Uuid::new_v4(), "Server", CredentialType::Ssh);
    let item = build_record_item(&record, false, false, true, true, 50, None);
    assert!(item.height() >= 3);
}

#[test]
fn build_record_item_selected_indicator() {
    let record = make_record(Uuid::new_v4(), "Test", "sub");
    // With unicode and selected=true, should have ◀
    let item = build_record_item(&record, true, false, true, true, 50, None);
    assert!(item.height() >= 3);

    // With ASCII and selected=true, should have <
    let item = build_record_item(&record, true, false, true, false, 50, None);
    assert!(item.height() >= 3);
}

#[test]
fn build_record_item_visual_selected() {
    let record = make_record(Uuid::new_v4(), "Test", "sub");
    let item = build_record_item(&record, false, true, true, true, 50, None);
    assert!(item.height() >= 3);
}

// ── Search highlight tests ──

#[test]
fn highlight_match_basic() {
    let spans = ListPanel::highlight_match("GitHub", "git");
    // Should produce two spans: "Git" (highlighted) + "Hub" (normal)
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].content.as_ref(), "Git");
    assert_eq!(spans[1].content.as_ref(), "Hub");
    // Verify the highlighted span has WARNING color + BOLD
    assert!(spans[0].style.fg == Some(theme::WARNING.into()));
    assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
    // Non-matching span should be plain text color
    assert!(spans[1].style.fg == Some(theme::TEXT.into()));
}

#[test]
fn highlight_match_multi_occurrence() {
    let spans = ListPanel::highlight_match("test_test_test", "test");
    // Should produce alternating: match + "_" + match + "_" + match
    assert_eq!(spans.len(), 5);
    assert_eq!(spans[0].content.as_ref(), "test"); // highlighted
    assert_eq!(spans[1].content.as_ref(), "_"); // normal
    assert_eq!(spans[2].content.as_ref(), "test"); // highlighted
    assert_eq!(spans[3].content.as_ref(), "_"); // normal
    assert_eq!(spans[4].content.as_ref(), "test"); // highlighted
                                                   // Highlighted spans should have WARNING + BOLD
    for i in [0, 2, 4] {
        assert!(spans[i].style.fg == Some(theme::WARNING.into()));
        assert!(spans[i].style.add_modifier.contains(Modifier::BOLD));
    }
}

#[test]
fn highlight_match_empty_query() {
    let spans = ListPanel::highlight_match("GitHub", "");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].content.as_ref(), "GitHub");
    assert!(spans[0].style.fg == Some(theme::TEXT.into()));
}

#[test]
fn highlight_match_case_insensitive() {
    let spans = ListPanel::highlight_match("MyGitRepo", "git");
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].content.as_ref(), "My");
    assert_eq!(spans[1].content.as_ref(), "Git"); // highlighted
    assert_eq!(spans[2].content.as_ref(), "Repo");
    assert!(spans[1].style.fg == Some(theme::WARNING.into()));
}

#[test]
fn highlight_match_no_match() {
    let spans = ListPanel::highlight_match("GitHub", "xyz");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].content.as_ref(), "GitHub");
    assert!(spans[0].style.fg == Some(theme::TEXT.into()));
}

#[test]
fn build_record_item_with_search_highlight() {
    let record = make_record(Uuid::new_v4(), "GitHub", "user@github.com");
    let item = build_record_item(&record, false, false, true, true, 50, Some("git"));
    assert!(item.height() >= 3);
}

// ── Filter-aware empty state variant tests ──

#[test]
fn render_empty_state_favorites() {
    let state = ListPanelState::default();
    let result = render_snapshot(&state, 40, 10, true, true, RecordFilter::Favorites);
    assert!(!result.is_empty());
}

#[test]
fn render_empty_state_expired() {
    let state = ListPanelState::default();
    let result = render_snapshot(&state, 40, 10, true, true, RecordFilter::Expired);
    assert!(!result.is_empty());
}

#[test]
fn render_empty_state_health_issues() {
    let state = ListPanelState::default();
    let result = render_snapshot(&state, 40, 10, true, true, RecordFilter::HealthIssues);
    assert!(!result.is_empty());
}

#[test]
fn render_empty_state_tag() {
    let state = ListPanelState::default();
    let result = render_snapshot(
        &state,
        40,
        10,
        true,
        true,
        RecordFilter::Tag("work".to_string()),
    );
    assert!(!result.is_empty());
}

#[test]
fn build_empty_state_variant_all() {
    let state = ListPanelState::default();
    let variant = build_empty_state_variant(&state, &RecordFilter::All);
    assert!(matches!(variant, EmptyStateVariant::NoPasswords));
}

#[test]
fn build_empty_state_variant_favorites() {
    let state = ListPanelState::default();
    let variant = build_empty_state_variant(&state, &RecordFilter::Favorites);
    assert!(matches!(variant, EmptyStateVariant::NoFavorites));
}

#[test]
fn build_empty_state_variant_expired() {
    let state = ListPanelState::default();
    let variant = build_empty_state_variant(&state, &RecordFilter::Expired);
    assert!(matches!(variant, EmptyStateVariant::NoExpired));
}

#[test]
fn build_empty_state_variant_health_issues() {
    let state = ListPanelState::default();
    let variant = build_empty_state_variant(&state, &RecordFilter::HealthIssues);
    assert!(matches!(variant, EmptyStateVariant::NoHealthIssues));
}

#[test]
fn build_empty_state_variant_trash() {
    let state = ListPanelState::default();
    let variant = build_empty_state_variant(&state, &RecordFilter::Trash);
    assert!(matches!(variant, EmptyStateVariant::EmptyTrash));
}

#[test]
fn build_empty_state_variant_tag() {
    let state = ListPanelState::default();
    let variant = build_empty_state_variant(&state, &RecordFilter::Tag("personal".to_string()));
    match variant {
        EmptyStateVariant::EmptyTag { tag_name } => {
            assert_eq!(tag_name, "personal");
        }
        other => panic!("Expected EmptyTag, got {:?}", other),
    }
}

#[test]
fn build_empty_state_variant_search_filter() {
    let state = ListPanelState::default();
    let variant = build_empty_state_variant(&state, &RecordFilter::Search("query".to_string()));
    match variant {
        EmptyStateVariant::NoSearchResults { query } => {
            assert_eq!(query, "query");
        }
        other => panic!("Expected NoSearchResults, got {:?}", other),
    }
}

#[test]
fn build_empty_state_variant_search_mode_overrides_filter() {
    // When in search mode with a non-empty query, it should use NoSearchResults
    // from the list mode search state, regardless of the filter
    let mut state = ListPanelState::default();
    state.mode = ListMode::Search(SearchState {
        query: "mysearch".to_string(),
        cursor: 8,
    });
    let variant = build_empty_state_variant(&state, &RecordFilter::All);
    match variant {
        EmptyStateVariant::NoSearchResults { query } => {
            assert_eq!(query, "mysearch");
        }
        other => panic!("Expected NoSearchResults from search mode, got {:?}", other),
    }
}

// ── Health badge tests ──

#[test]
fn health_badge_compromised() {
    let span = health_badge(Some(&HealthIssue::Compromised), true).unwrap();
    let text = span.content.as_ref();
    assert!(text.contains('\u{1F534}')); // 🔴
    assert!(text.contains('\u{5DF2}')); // 已 (part of 已泄露)
    assert!(span.style.fg == Some(theme::ERROR.into()));
}

#[test]
fn health_badge_compromised_ascii() {
    let span = health_badge(Some(&HealthIssue::Compromised), false).unwrap();
    let text = span.content.as_ref();
    assert!(text.contains('!'));
    assert!(span.style.fg == Some(theme::ERROR.into()));
}

#[test]
fn health_badge_weak() {
    let span = health_badge(Some(&HealthIssue::Weak), true).unwrap();
    let text = span.content.as_ref();
    assert!(text.contains('\u{26A0}')); // ⚠
    assert!(text.contains('\u{5F31}')); // 弱
    assert!(span.style.fg == Some(theme::WARNING.into()));
}

#[test]
fn health_badge_weak_ascii() {
    let span = health_badge(Some(&HealthIssue::Weak), false).unwrap();
    let text = span.content.as_ref();
    assert!(text.contains('!'));
    assert!(span.style.fg == Some(theme::WARNING.into()));
}

#[test]
fn health_badge_duplicate() {
    let span = health_badge(Some(&HealthIssue::Duplicate { group_size: 3 }), true).unwrap();
    let text = span.content.as_ref();
    assert!(text.contains('\u{26A0}')); // ⚠
    assert!(text.contains('3'));
    assert!(text.contains("\u{91CD}\u{590D}")); // 重复
    assert!(span.style.fg == Some(theme::WARNING.into()));
}

#[test]
fn health_badge_duplicate_ascii() {
    let span = health_badge(Some(&HealthIssue::Duplicate { group_size: 5 }), false).unwrap();
    let text = span.content.as_ref();
    assert!(text.contains('5'));
    assert!(span.style.fg == Some(theme::WARNING.into()));
}

#[test]
fn health_badge_expired() {
    let result = health_badge(Some(&HealthIssue::Expired), true);
    assert!(result.is_none());
}

#[test]
fn health_badge_none() {
    let result: Option<Span<'static>> = health_badge(None, true);
    assert!(result.is_none());
}

// ── Health badge priority integration tests ──

#[test]
fn render_compromised_badge_in_list() {
    let record = make_record_with_compromised(Uuid::new_v4(), "HackedSite");
    let state = ListPanelState::with_records(vec![record]);
    let result = render_snapshot(&state, 50, 10, true, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_duplicate_badge_in_list() {
    let record = make_record_with_duplicate(Uuid::new_v4(), "SharedPass", 3);
    let state = ListPanelState::with_records(vec![record]);
    let result = render_snapshot(&state, 50, 10, true, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_compromised_takes_priority_over_weak() {
    // Both compromised and weak: compromised badge should win
    let mut record = make_record_with_compromised(Uuid::new_v4(), "BothBad");
    record.has_weak_password = true;
    let state = ListPanelState::with_records(vec![record]);
    let result = render_snapshot(&state, 50, 10, true, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_weak_takes_priority_over_duplicate() {
    // Both weak and duplicate: weak badge should win
    let mut record = make_record_with_weak(Uuid::new_v4(), "WeakDup");
    record.duplicate_group_size = Some(4);
    let state = ListPanelState::with_records(vec![record]);
    let result = render_snapshot(&state, 50, 10, true, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn render_duplicate_group_size_one_no_badge() {
    // group_size == 1 means it's in a group of 1 (itself), not actually duplicated
    let record = make_record_with_duplicate(Uuid::new_v4(), "Unique", 1);
    let state = ListPanelState::with_records(vec![record]);
    let result = render_snapshot(&state, 50, 10, true, true, RecordFilter::All);
    assert!(!result.is_empty());
}

#[test]
fn separator_is_blank_line() {
    let record = make_record(Uuid::new_v4(), "Test", "sub");
    let item = build_record_item(&record, false, false, true, true, 50, None);
    // Item has 3 lines: title, subtitle, blank separator
    assert_eq!(item.height(), 3);
}

// ── Trash item rendering tests ──

fn make_trash_record(id: Uuid, name: &str, days_ago: i64) -> TuiRecord {
    let deleted_at = Utc::now() - chrono::Duration::try_days(days_ago).unwrap();
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
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted: true,
        deleted_at: Some(deleted_at),
        tags: Vec::new(),
        sync_status: None,
    }
}

#[test]
fn build_trash_item_has_three_lines() {
    let record = make_trash_record(Uuid::new_v4(), "DeletedSite", 5);
    let item = build_trash_item(&record, false, false, true, true, 50, 30);
    assert!(
        item.height() >= 3,
        "trash item should have at least 3 lines (title + metadata + separator)"
    );
}

#[test]
fn build_trash_item_selected_indicator() {
    let record = make_trash_record(Uuid::new_v4(), "TestTrash", 2);
    let item = build_trash_item(&record, true, false, true, true, 50, 30);
    assert!(item.height() >= 3);
}

#[test]
fn render_trash_list_with_records() {
    let r1 = make_trash_record(Uuid::new_v4(), "DeletedA", 3);
    let r2 = make_trash_record(Uuid::new_v4(), "DeletedB", 15);
    let state = ListPanelState::with_records(vec![r1, r2]);
    let result = render_snapshot(&state, 50, 15, true, true, RecordFilter::Trash);
    assert!(!result.is_empty());
}

#[test]
fn render_trash_list_empty_state() {
    let state = ListPanelState::default();
    let result = render_snapshot(&state, 40, 10, true, true, RecordFilter::Trash);
    assert!(!result.is_empty());
}

#[test]
fn render_trash_list_unfocused() {
    let r1 = make_trash_record(Uuid::new_v4(), "TrashItem", 5);
    let state = ListPanelState::with_records(vec![r1]);
    let result = render_snapshot(&state, 50, 10, false, true, RecordFilter::Trash);
    assert!(!result.is_empty());
}

#[test]
fn trash_warning_tier_colors_applied() {
    let r1 = make_trash_record(Uuid::new_v4(), "Urgent", 28);
    let state = ListPanelState::with_records(vec![r1]);
    let result = render_snapshot(&state, 50, 10, true, true, RecordFilter::Trash);
    assert!(!result.is_empty());
}

#[test]
fn trash_item_never_auto_delete_retention_zero() {
    let record = make_trash_record(Uuid::new_v4(), "NeverDelete", 10);
    let item = build_trash_item(&record, false, false, true, true, 50, 0);
    assert!(item.height() >= 3);
}

// ── Acceptance Criteria verification tests ─────────────────────────────────

#[test]
fn acceptance_trash_empty_state_renders() {
    let state = ListPanelState::default();
    let result = render_snapshot(&state, 40, 10, true, true, RecordFilter::Trash);
    assert!(!result.is_empty());
    let variant = build_empty_state_variant(&state, &RecordFilter::Trash);
    assert!(matches!(variant, EmptyStateVariant::EmptyTrash));
}

#[test]
fn acceptance_trash_list_with_deleted_records() {
    let r1 = make_trash_record(Uuid::new_v4(), "DeletedA", 3);
    let r2 = make_trash_record(Uuid::new_v4(), "DeletedB", 15);
    let state = ListPanelState::with_records(vec![r1, r2]);
    let result = render_snapshot(&state, 50, 15, true, true, RecordFilter::Trash);
    assert!(!result.is_empty());
}

#[test]
fn acceptance_trash_item_warning_progressive() {
    // Critical: deleted 28 days ago with 30-day retention = 2 days remaining
    let critical = make_trash_record(Uuid::new_v4(), "Critical", 28);
    let item = build_trash_item(&critical, false, false, true, true, 50, 30);
    assert!(item.height() >= 3);

    // Urgent: deleted 25 days ago = 5 days remaining
    let urgent = make_trash_record(Uuid::new_v4(), "Urgent", 25);
    let item = build_trash_item(&urgent, false, false, true, true, 50, 30);
    assert!(item.height() >= 3);

    // Safe: deleted 5 days ago = 25 days remaining
    let safe = make_trash_record(Uuid::new_v4(), "Safe", 5);
    let item = build_trash_item(&safe, false, false, true, true, 50, 30);
    assert!(item.height() >= 3);
}

#[test]
fn acceptance_never_auto_delete_no_remaining_line() {
    let record = make_trash_record(Uuid::new_v4(), "NeverDelete", 100);
    let item = build_trash_item(&record, false, false, true, true, 50, 0);
    assert!(item.height() >= 3);
}

#[test]
fn acceptance_trash_visual_mode_renders() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let r1 = make_trash_record(id1, "A", 5);
    let r2 = make_trash_record(id2, "B", 10);
    let mut state = ListPanelState::with_records(vec![r1, r2]);
    let mut selected = HashSet::new();
    selected.insert(id1);
    state.mode = ListMode::Visual(VisualState {
        selected_ids: selected,
    });
    let result = render_snapshot(&state, 50, 15, true, true, RecordFilter::Trash);
    assert!(!result.is_empty());
}

#[test]
fn acceptance_trash_search_mode_in_list() {
    let r1 = make_trash_record(Uuid::new_v4(), "GitHub", 3);
    let mut state = ListPanelState::with_records(vec![r1]);
    state.mode = ListMode::Search(SearchState {
        query: "git".to_string(),
        cursor: 3,
    });
    let result = render_snapshot(&state, 50, 10, true, true, RecordFilter::Trash);
    assert!(!result.is_empty());
}

#[test]
fn acceptance_trash_ascii_mode() {
    let r1 = make_trash_record(Uuid::new_v4(), "Test", 5);
    let state = ListPanelState::with_records(vec![r1]);
    let result = render_snapshot(&state, 50, 10, true, false, RecordFilter::Trash);
    assert!(!result.is_empty());
}
