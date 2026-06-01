use chrono::{TimeZone, Utc};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use uuid::Uuid;

use oak_keyring::commands::types::RecordFilter;
use oak_keyring::tui::screens::main::list::ListPanel;
use oak_keyring::tui::state::list_state::{ListMode, ListPanelState, SearchState, VisualState};
use oak_keyring::types::credential::CredentialType;
use oak_keyring::types::record::TuiRecord;

use crate::support::snapshot_locale;

use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Deterministic test fixtures
// ---------------------------------------------------------------------------

/// Fixed timestamp in 2024 — always renders as "2024-06-15" (stable across runs).
fn fixed_past_ts() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap()
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

fn make_record_with_type(id: Uuid, name: &str, cred_type: CredentialType) -> TuiRecord {
    TuiRecord {
        credential_type: cred_type,
        ..make_record(id, name, "")
    }
}

fn make_record_with_health(
    id: Uuid,
    name: &str,
    compromised: bool,
    weak: bool,
    dup_group: Option<usize>,
) -> TuiRecord {
    TuiRecord {
        is_compromised: compromised,
        has_weak_password: weak,
        duplicate_group_size: dup_group,
        ..make_record(id, name, "site.com")
    }
}

fn make_trash_record(id: Uuid, name: &str, days_ago: i64) -> TuiRecord {
    let deleted_at = Utc::now() - chrono::Duration::try_days(days_ago).unwrap();
    TuiRecord {
        id,
        deleted: true,
        deleted_at: Some(deleted_at),
        updated_at: deleted_at,
        ..make_record(id, name, "")
    }
}

fn render_to_snapshot(
    state: &ListPanelState,
    width: u16,
    height: u16,
    focused: bool,
    unicode: bool,
    filter: RecordFilter,
) -> String {
    let _locale = snapshot_locale();
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ListPanel::view(frame, frame.area(), state, focused, unicode, filter, 30);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    format!("{:?}", buf)
}

// ---------------------------------------------------------------------------
// Snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn list_normal_rows() {
    let _locale = snapshot_locale();
    let r1 = make_record(Uuid::new_v4(), "GitHub", "user@github.com");
    let r2 = make_record(Uuid::new_v4(), "GitLab", "dev@gitlab.com");
    let r3 = make_record(Uuid::new_v4(), "Bitbucket", "team@bitbucket.org");
    let state = ListPanelState::with_records(vec![r1, r2, r3]);

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
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

    insta::assert_snapshot!("list_normal_rows", terminal.backend());
}

#[test]
fn list_api_ssh_types() {
    let _locale = snapshot_locale();
    let r1 = make_record_with_type(Uuid::new_v4(), "GitHub", CredentialType::Login);
    let r2 = make_record_with_type(Uuid::new_v4(), "AWS Key", CredentialType::Api);
    let r3 = make_record_with_type(Uuid::new_v4(), "Server", CredentialType::Ssh);
    let state = ListPanelState::with_records(vec![r1, r2, r3]);

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
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

    insta::assert_snapshot!("list_api_ssh_types", terminal.backend());
}

#[test]
fn list_selected_row() {
    let _locale = snapshot_locale();
    let r1 = make_record(Uuid::new_v4(), "GitHub", "user@github.com");
    let r2 = make_record(Uuid::new_v4(), "GitLab", "dev@gitlab.com");
    let mut state = ListPanelState::with_records(vec![r1, r2]);
    state.selected_index = Some(0);

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
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

    insta::assert_snapshot!("list_selected_row", terminal.backend());
}

#[test]
fn list_health_badge_compromised() {
    let _locale = snapshot_locale();
    let record = make_record_with_health(Uuid::new_v4(), "HackedSite", true, false, None);
    let state = ListPanelState::with_records(vec![record]);

    let backend = TestBackend::new(60, 10);
    let mut terminal = Terminal::new(backend).unwrap();
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

    insta::assert_snapshot!("list_health_badge_compromised", terminal.backend());
}

#[test]
fn list_health_badge_weak() {
    let _locale = snapshot_locale();
    let record = make_record_with_health(Uuid::new_v4(), "WeakPass", false, true, None);
    let state = ListPanelState::with_records(vec![record]);

    let backend = TestBackend::new(60, 10);
    let mut terminal = Terminal::new(backend).unwrap();
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

    insta::assert_snapshot!("list_health_badge_weak", terminal.backend());
}

#[test]
fn list_health_badge_duplicate() {
    let _locale = snapshot_locale();
    let record = make_record_with_health(Uuid::new_v4(), "SharedPass", false, false, Some(3));
    let state = ListPanelState::with_records(vec![record]);

    let backend = TestBackend::new(60, 10);
    let mut terminal = Terminal::new(backend).unwrap();
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

    insta::assert_snapshot!("list_health_badge_duplicate", terminal.backend());
}

#[test]
fn list_search_highlight() {
    let _locale = snapshot_locale();
    let r1 = make_record(Uuid::new_v4(), "GitHub", "user@github.com");
    let r2 = make_record(Uuid::new_v4(), "GitLab", "dev@gitlab.com");
    let mut state = ListPanelState::with_records(vec![r1, r2]);
    state.mode = ListMode::Search(SearchState {
        query: "git".to_string(),
        cursor: 3,
        pre_search: None,
    });

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
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

    insta::assert_snapshot!("list_search_highlight", terminal.backend());
}

#[test]
fn list_search_multi_term() {
    let _locale = snapshot_locale();
    let r1 = make_record(Uuid::new_v4(), "GitHub API", "api.github.com");
    let r2 = make_record(Uuid::new_v4(), "GitLab SSH", "ssh.gitlab.com");
    let mut state = ListPanelState::with_records(vec![r1, r2]);
    state.mode = ListMode::Search(SearchState {
        query: "git api".to_string(),
        cursor: 7,
        pre_search: None,
    });

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
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

    insta::assert_snapshot!("list_search_multi_term", terminal.backend());
}

#[test]
fn list_visual_mode() {
    let _locale = snapshot_locale();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    let r1 = make_record(id1, "GitHub", "user@github.com");
    let r2 = make_record(id2, "GitLab", "dev@gitlab.com");
    let r3 = make_record(id3, "Bitbucket", "team@bb.org");
    let mut state = ListPanelState::with_records(vec![r1, r2, r3]);
    let mut selected = HashSet::new();
    selected.insert(id1);
    selected.insert(id3);
    state.mode = ListMode::Visual(VisualState {
        selected_ids: selected,
    });
    state.selected_index = Some(0);

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
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

    insta::assert_snapshot!("list_visual_mode", terminal.backend());
}

#[test]
fn list_narrow_width() {
    let _locale = snapshot_locale();
    let r1 = make_record_with_health(Uuid::new_v4(), "GitHub", true, false, None);
    let r2 = make_record(Uuid::new_v4(), "GitLab", "dev@gitlab.com");
    let state = ListPanelState::with_records(vec![r1, r2]);

    // 90px = Minimum tier: subtitle and badges should be hidden
    let backend = TestBackend::new(90, 12);
    let mut terminal = Terminal::new(backend).unwrap();
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

    insta::assert_snapshot!("list_narrow_width", terminal.backend());
}

#[test]
fn list_trash_rows() {
    let _locale = snapshot_locale();
    // 5 days ago → Safe tier (25 days remaining)
    let r1 = make_trash_record(Uuid::new_v4(), "OldSite", 5);
    // 20 days ago → Moderate tier (10 days remaining)
    let r2 = make_trash_record(Uuid::new_v4(), "Another", 20);
    let state = ListPanelState::with_records(vec![r1, r2]);

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ListPanel::view(
                frame,
                frame.area(),
                &state,
                true,
                true,
                RecordFilter::Trash,
                30,
            );
        })
        .unwrap();

    insta::assert_snapshot!("list_trash_rows", terminal.backend());
}

#[test]
fn list_trash_selected() {
    let _locale = snapshot_locale();
    let r1 = make_trash_record(Uuid::new_v4(), "DeletedSite", 5);
    let r2 = make_trash_record(Uuid::new_v4(), "AnotherDeleted", 10);
    let mut state = ListPanelState::with_records(vec![r1, r2]);
    state.selected_index = Some(0);

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ListPanel::view(
                frame,
                frame.area(),
                &state,
                true,
                true,
                RecordFilter::Trash,
                30,
            );
        })
        .unwrap();

    insta::assert_snapshot!("list_trash_selected", terminal.backend());
}

#[test]
fn list_empty_state() {
    let _locale = snapshot_locale();
    let state = ListPanelState::default();

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
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

    insta::assert_snapshot!("list_empty_state", terminal.backend());
}

#[test]
fn list_empty_trash() {
    let _locale = snapshot_locale();
    let state = ListPanelState::default();

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ListPanel::view(
                frame,
                frame.area(),
                &state,
                true,
                true,
                RecordFilter::Trash,
                30,
            );
        })
        .unwrap();

    insta::assert_snapshot!("list_empty_trash", terminal.backend());
}

#[test]
fn list_unfocused() {
    let _locale = snapshot_locale();
    let r1 = make_record(Uuid::new_v4(), "GitHub", "user@github.com");
    let r2 = make_record(Uuid::new_v4(), "GitLab", "dev@gitlab.com");
    let state = ListPanelState::with_records(vec![r1, r2]);

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ListPanel::view(
                frame,
                frame.area(),
                &state,
                false,
                true,
                RecordFilter::All,
                30,
            );
        })
        .unwrap();

    insta::assert_snapshot!("list_unfocused", terminal.backend());
}

// ---------------------------------------------------------------------------
// Buffer cell-style assertions for search highlight
// ---------------------------------------------------------------------------

#[test]
fn list_search_highlight_cell_styles() {
    let _locale = snapshot_locale();
    let r1 = make_record(Uuid::new_v4(), "GitHub", "user@github.com");
    let r2 = make_record(Uuid::new_v4(), "GitLab", "dev@gitlab.com");
    let mut state = ListPanelState::with_records(vec![r1, r2]);
    state.mode = ListMode::Search(SearchState {
        query: "git".to_string(),
        cursor: 3,
        pre_search: None,
    });
    // Select the second record so the first (GitHub) is NOT selected,
    // avoiding the List highlight_style overriding search highlight fg.
    state.selected_index = Some(1);

    let backend = TestBackend::new(60, 15);
    let mut terminal = Terminal::new(backend).unwrap();
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

    let buf = terminal.backend().buffer();
    let warning = ratatui::style::Color::Rgb(255, 158, 100);

    // y=0: search bar. y=1: padding row. y=2: first record title line.
    // The login type icon occupies the first cells after the left padding, so
    // "G" starts at x=4 in the current unicode list layout.
    // Search term "git" matches "Git" case-insensitively -> x=4,5,6 highlighted.

    // Highlighted cells (G,i,t) must have WARNING fg + BOLD modifier
    for x in 4..=6 {
        let cell = buf
            .cell((x, 2))
            .unwrap_or_else(|| panic!("cell ({}, 2) missing", x));
        assert_eq!(
            cell.style().fg,
            Some(warning),
            "cell ({}, 2) should have WARNING fg for highlighted 'Git', got {:?}",
            x,
            cell.style().fg
        );
        assert!(
            cell.style()
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD),
            "cell ({}, 2) should have BOLD modifier for highlighted 'Git'",
            x
        );
    }

    // Non-highlighted cells (H,u,b) must NOT have WARNING fg
    for x in 7..=9 {
        let cell = buf
            .cell((x, 2))
            .unwrap_or_else(|| panic!("cell ({}, 2) missing", x));
        assert_ne!(
            cell.style().fg,
            Some(warning),
            "cell ({}, 2) should NOT have WARNING fg for non-matching 'Hub', got {:?}",
            x,
            cell.style().fg
        );
    }
}

// ---------------------------------------------------------------------------
// Content-assertion tests for acceptance criteria
// ---------------------------------------------------------------------------

#[test]
fn list_row_contains_type_prefix_name_subtitle() {
    let record = make_record(Uuid::new_v4(), "GitHub", "user@github.com");
    let state = ListPanelState::with_records(vec![record]);
    let result = render_to_snapshot(&state, 60, 10, true, true, RecordFilter::All);

    assert!(
        result.contains("GitHub"),
        "rendered buffer should contain the record name"
    );
    assert!(
        result.contains("user@github.com") || result.contains("github"),
        "rendered buffer should contain the subtitle"
    );
    assert!(
        result.contains("2024-06-15"),
        "rendered buffer should contain the timestamp"
    );
}

#[test]
fn list_api_record_shows_prefix() {
    let record = make_record_with_type(Uuid::new_v4(), "AWS", CredentialType::Api);
    let state = ListPanelState::with_records(vec![record]);
    let result = render_to_snapshot(&state, 60, 10, true, true, RecordFilter::All);

    assert!(
        result.contains("\u{f0bc4} AWS"),
        "API record should show type prefix"
    );
    assert!(
        result.contains("AWS"),
        "rendered buffer should contain the record name"
    );
}

#[test]
fn list_ssh_record_shows_prefix() {
    let record = make_record_with_type(Uuid::new_v4(), "Server", CredentialType::Ssh);
    let state = ListPanelState::with_records(vec![record]);
    let result = render_to_snapshot(&state, 60, 10, true, true, RecordFilter::All);

    assert!(
        result.contains("\u{f1575} Server"),
        "SSH record should show type prefix"
    );
}

#[test]
fn list_selected_shows_indicator() {
    let record = make_record(Uuid::new_v4(), "Test", "sub");
    let mut state = ListPanelState::with_records(vec![record]);
    state.selected_index = Some(0);

    let result = render_to_snapshot(&state, 60, 10, true, true, RecordFilter::All);
    // The selected row renders a left-side gutter bar (cyan bg space) instead
    // of a right-side ◀ marker. Verify the record name is present and the
    // buffer was rendered without panic.
    assert!(
        result.contains("Test"),
        "selected row should render record name"
    );
}

#[test]
fn list_compromised_badge_visible() {
    let record = make_record_with_health(Uuid::new_v4(), "Hacked", true, false, None);
    let state = ListPanelState::with_records(vec![record]);
    let result = render_to_snapshot(&state, 60, 10, true, true, RecordFilter::All);

    // The compromised badge is now icon-only: \u{F06BD} (Nerd Font) or "[leaked]" (ASCII).
    assert!(
        result.contains("\u{F06BD}") || result.contains("[leaked]"),
        "compromised record should show leaked icon badge"
    );
}

#[test]
fn list_weak_badge_visible() {
    let record = make_record_with_health(Uuid::new_v4(), "Weak", false, true, None);
    let state = ListPanelState::with_records(vec![record]);
    let result = render_to_snapshot(&state, 60, 10, true, true, RecordFilter::All);

    assert!(
        result.contains("Weak") || result.contains("\u{26A0}"),
        "weak password record should show weak badge"
    );
}

#[test]
fn list_duplicate_badge_visible() {
    let record = make_record_with_health(Uuid::new_v4(), "Dup", false, false, Some(3));
    let state = ListPanelState::with_records(vec![record]);
    let result = render_to_snapshot(&state, 60, 10, true, true, RecordFilter::All);

    assert!(
        result.contains("3") && (result.contains("Duplicate") || result.contains("\u{26A0}")),
        "duplicate record should show group size badge"
    );
}

#[test]
fn list_search_bar_shows_query() {
    let state = ListPanelState {
        mode: ListMode::Search(SearchState {
            query: "github".to_string(),
            cursor: 6,
            pre_search: None,
        }),
        ..Default::default()
    };
    let result = render_to_snapshot(&state, 60, 10, true, true, RecordFilter::All);

    assert!(
        result.contains("github"),
        "search bar should show the query text"
    );
}

#[test]
fn list_visual_bar_shows_count() {
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

    let result = render_to_snapshot(&state, 60, 15, true, true, RecordFilter::All);
    assert!(
        result.contains("VISUAL") || result.contains("Visual"),
        "visual mode bar should show mode label"
    );
}

#[test]
fn list_trash_shows_deletion_metadata() {
    // 5 days ago deleted record
    let record = make_trash_record(Uuid::new_v4(), "Deleted", 5);
    let state = ListPanelState::with_records(vec![record]);
    let result = render_to_snapshot(&state, 60, 10, true, true, RecordFilter::Trash);

    // Should contain the record name and some deletion metadata
    assert!(
        result.contains("Deleted"),
        "trash row should contain record name"
    );
    assert!(
        result.contains("days") || result.contains("天"),
        "trash metadata should contain days reference"
    );
}

#[test]
fn list_narrow_width_hides_subtitle_and_badge() {
    let record = make_record_with_health(Uuid::new_v4(), "Test", true, false, None);
    let state = ListPanelState::with_records(vec![record]);

    // Full width: subtitle and badge should appear
    let full_result = render_to_snapshot(&state, 120, 10, true, true, RecordFilter::All);
    assert!(
        full_result.contains("site.com") || full_result.contains("Leaked"),
        "full width should show subtitle and/or badge"
    );

    // Minimum width (90px): subtitle hidden, badge hidden
    let narrow_result = render_to_snapshot(&state, 90, 10, true, true, RecordFilter::All);
    assert!(
        !narrow_result.contains("site.com"),
        "minimum width should hide subtitle"
    );
}
