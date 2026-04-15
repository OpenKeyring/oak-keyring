//! List panel rendering for the main screen (U3 spec).
//!
//! Renders:
//! - Sort/search/visual-mode bar at top (1 line)
//! - Two-line list items with type prefix, name, health badge, timestamp, separator
//! - Empty state fallback when no records are present

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::commands::types::{SortDirection, SortField};
use crate::tui::components::empty_state::{EmptyStateVariant, EmptyStateWidget};
use crate::tui::state::list_state::{
    format_relative_time, format_type_prefix, ListMode, ListPanelState,
};
use crate::tui::theme;

/// Panel responsible for rendering the password list.
pub struct ListPanel;

impl ListPanel {
    /// Render the list panel.
    ///
    /// # Arguments
    /// * `frame` - The ratatui frame to render into.
    /// * `area` - The rectangular area allocated to the list panel.
    /// * `state` - The current list panel state (records, selection, mode, sort).
    /// * `focused` - Whether the list panel currently has keyboard focus.
    /// * `unicode` - Whether to use unicode characters (vs ASCII fallbacks).
    /// * `is_trash_view` - Whether the current filter is Trash.
    pub fn view(
        frame: &mut Frame,
        area: Rect,
        state: &ListPanelState,
        focused: bool,
        unicode: bool,
        is_trash_view: bool,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Split: bar (1 line) + list (remaining)
        let bar_height = 1u16;
        let bar_area = Rect::new(area.x, area.y, area.width, bar_height);
        let list_area = Rect::new(
            area.x,
            area.y + bar_height,
            area.width,
            area.height.saturating_sub(bar_height),
        );

        // 1. Render the bar
        let bar_content = build_bar_content(state, unicode);
        let bar = Paragraph::new(bar_content).style(Style::default().fg(theme::TEXT));
        frame.render_widget(bar, bar_area);

        // 2. Render list or empty state
        if state.records.is_empty() {
            render_empty_state(frame, list_area, state, unicode, is_trash_view);
        } else {
            render_list(frame, list_area, state, focused, unicode);
        }
    }
}

// ---------------------------------------------------------------------------
// Bar rendering
// ---------------------------------------------------------------------------

/// Build the bar content based on the current list mode.
fn build_bar_content<'a>(state: &ListPanelState, unicode: bool) -> Line<'a> {
    match &state.mode {
        ListMode::Normal => build_sort_bar(&state.sort.field, &state.sort.direction, unicode),
        ListMode::Search(search_state) => build_search_bar(&search_state.query, unicode),
        ListMode::Visual(visual_state) => build_visual_bar(visual_state.selected_ids.len()),
    }
}

/// Build sort bar: `  排序: [ 排序字段 ▼ ]  [ ↑/↓ 升序/降序 ]`
fn build_sort_bar<'a>(field: &SortField, direction: &SortDirection, unicode: bool) -> Line<'a> {
    let field_name = sort_field_label(field);
    let (dir_icon, dir_label) = sort_direction_label(direction, unicode);
    let down_icon = if unicode { "\u{25BC}" } else { "v" }; // ▼ / v

    Line::from(vec![
        Span::raw("  \u{6392}\u{5E8F}: [ "), // "  排序: [ "
        Span::styled(
            format!("{} {}", field_name, down_icon),
            Style::default().fg(theme::BRAND),
        ),
        Span::raw(" ]  [ "),
        Span::styled(
            format!("{} {}", dir_icon, dir_label),
            Style::default().fg(theme::BRAND),
        ),
        Span::raw(" ]"),
    ])
}

/// Build search bar: `  🔍 搜索: <query>_`
fn build_search_bar<'a>(query: &str, unicode: bool) -> Line<'a> {
    let search_icon = if unicode { "\u{1F50D}" } else { ">" }; // 🔍 / >
    let display_query = format!("{} \u{641C}\u{7D22}: {}_", search_icon, query); // "🔍 搜索: <query>_"

    Line::from(vec![
        Span::styled(
            format!("  {}", display_query),
            Style::default().fg(theme::TEXT),
        ),
    ])
}

/// Build visual mode bar: `  多选模式 (N 已选)` in BRAND bold
fn build_visual_bar<'a>(selected_count: usize) -> Line<'a> {
    Line::from(vec![Span::styled(
        format!(
            "  \u{591A}\u{9009}\u{6A21}\u{5F0F} ({} \u{5DF2}\u{9009})", // "  多选模式 (N 已选)"
            selected_count
        ),
        Style::default()
            .fg(theme::BRAND)
            .add_modifier(Modifier::BOLD),
    )])
}

/// Return the Chinese display label for a sort field.
fn sort_field_label(field: &SortField) -> &'static str {
    match field {
        SortField::CreatedAt => "\u{521B}\u{5EFA}\u{65F6}\u{95F4}",  // 创建时间
        SortField::UpdatedAt => "\u{66F4}\u{65B0}\u{65F6}\u{95F4}",  // 更新时间
        SortField::Name => "\u{540D}\u{79F0}",                        // 名称
        SortField::UsageFrequency => "\u{4F7F}\u{7528}\u{9891}\u{7387}", // 使用频率
    }
}

/// Return the icon and Chinese label for a sort direction.
fn sort_direction_label(direction: &SortDirection, unicode: bool) -> (&'static str, &'static str) {
    match direction {
        SortDirection::Desc => {
            if unicode {
                ("\u{2193}", "\u{964D}\u{5E8F}") // ↓ 降序
            } else {
                ("v", "\u{964D}\u{5E8F}")
            }
        }
        SortDirection::Asc => {
            if unicode {
                ("\u{2191}", "\u{5347}\u{5E8F}") // ↑ 升序
            } else {
                ("^", "\u{5347}\u{5E8F}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// List rendering
// ---------------------------------------------------------------------------

/// Render the scrollable record list.
fn render_list(
    frame: &mut Frame,
    area: Rect,
    state: &ListPanelState,
    focused: bool,
    unicode: bool,
) {
    let visual_ids = match &state.mode {
        ListMode::Visual(vs) => Some(&vs.selected_ids),
        _ => None,
    };

    let items: Vec<ListItem<'_>> = state
        .records
        .iter()
        .enumerate()
        .map(|(idx, record)| {
            let is_selected = state.selected_index == Some(idx);
            let is_visual_selected = visual_ids
                .map_or(false, |ids| ids.contains(&record.id));
            build_record_item(record, is_selected, is_visual_selected, focused, unicode, area.width)
        })
        .collect();

    let highlight_style = if focused {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::DIM)
    };

    let list = List::new(items).highlight_style(highlight_style);

    let mut list_state = ListState::default();
    list_state.select(state.selected_index);

    frame.render_stateful_widget(list, area, &mut list_state);
}

/// Build a single two-line list item with separator.
///
/// Line 1 (title): `  [Type] Name [⚠弱]    timestamp ◀`
/// Line 2 (subtitle): `  subtitle`
/// Line 3 (separator): `─────` or `-----`
fn build_record_item<'a>(
    record: &crate::types::record::TuiRecord,
    is_selected: bool,
    is_visual_selected: bool,
    focused: bool,
    unicode: bool,
    area_width: u16,
) -> ListItem<'a> {
    let indicator = if unicode { " \u{25C0}" } else { " <" }; // ◀ / <

    // ── Line 1: Title ──
    let type_prefix = format_type_prefix(&record.credential_type);
    let timestamp = format_relative_time(&record.updated_at);

    // Calculate padding between name+badge and timestamp
    let name_part = format!("  {}{}", type_prefix, record.name);
    let badge_part = if record.has_weak_password {
        let badge = if unicode { " \u{26A0}\u{5F31}" } else { " !weak" }; // ⚠弱 / !weak
        badge.to_string()
    } else {
        String::new()
    };
    let right_part = format!("{}{}", timestamp, if is_selected { indicator } else { "" });

    let padding_len = (area_width as usize)
        .saturating_sub(name_part.chars().count())
        .saturating_sub(badge_part.chars().count())
        .saturating_sub(right_part.chars().count());

    let base_style = if is_visual_selected {
        Style::default()
            .bg(theme::BRAND)
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
    } else if is_selected && focused {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT)
    };

    let badge_style = if is_visual_selected {
        Style::default()
            .bg(theme::BRAND)
            .fg(theme::WARNING)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::WARNING)
    };

    let title_spans = vec![
        Span::styled(name_part, base_style),
        Span::styled(badge_part, badge_style),
        Span::styled(" ".repeat(padding_len), base_style),
        Span::styled(right_part, base_style),
    ];

    // For visual-selected items, override all spans to use BRAND bg
    // This is already handled above via is_visual_selected branch

    let title_line = Line::from(title_spans);

    // ── Line 2: Subtitle ──
    let subtitle_style = if is_visual_selected {
        Style::default()
            .bg(theme::BRAND)
            .fg(theme::TEXT_SECONDARY)
    } else if is_selected && focused {
        Style::default()
            .fg(theme::TEXT_SECONDARY)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    let subtitle_line = Line::from(Span::styled(
        format!("  {}", record.subtitle),
        subtitle_style,
    ));

    // ── Line 3: Separator ──
    let sep_char = if unicode { '\u{2500}' } else { '-' }; // ─ / -
    let sep_text: String = std::iter::repeat_n(sep_char, area_width as usize).collect();
    let separator_line = Line::from(Span::styled(
        sep_text,
        Style::default().fg(theme::BORDER),
    ));

    ListItem::new(vec![title_line, subtitle_line, separator_line])
}

// ---------------------------------------------------------------------------
// Empty state
// ---------------------------------------------------------------------------

/// Render empty state when there are no records.
fn render_empty_state(
    frame: &mut Frame,
    area: Rect,
    state: &ListPanelState,
    unicode: bool,
    is_trash_view: bool,
) {
    let variant = match &state.mode {
        ListMode::Search(search_state) if !search_state.query.is_empty() => {
            EmptyStateVariant::NoSearchResults {
                query: search_state.query.clone(),
            }
        }
        _ if is_trash_view => EmptyStateVariant::EmptyTrash,
        _ => EmptyStateVariant::NoPasswords,
    };

    EmptyStateWidget::view(frame, area, &variant, unicode);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::list_state::{SearchState, VisualState};
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;
    use chrono::Utc;
    use ratatui::backend::TestBackend;
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted: false,
            deleted_at: None,
            tags: Vec::new(),
            sync_status: None,
        }
    }

    fn make_record_with_type(
        id: Uuid,
        name: &str,
        cred_type: CredentialType,
    ) -> TuiRecord {
        TuiRecord {
            id,
            credential_type: cred_type,
            name: name.to_string(),
            subtitle: String::new(),
            is_favorite: false,
            is_expired: false,
            expires_at: None,
            has_weak_password: false,
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
        is_trash_view: bool,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                ListPanel::view(frame, frame.area(), state, focused, unicode, is_trash_view);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        format!("{:?}", buf)
    }

    #[test]
    fn render_empty_state_no_passwords() {
        let state = ListPanelState::default();
        let result = render_snapshot(&state, 40, 10, true, true, false);
        // Should render without panicking and contain no-passwords empty state
        assert!(!result.is_empty());
    }

    #[test]
    fn render_empty_state_trash() {
        let state = ListPanelState::default();
        let result = render_snapshot(&state, 40, 10, true, true, true);
        assert!(!result.is_empty());
    }

    #[test]
    fn render_empty_state_search_no_results() {
        let mut state = ListPanelState::default();
        state.mode = ListMode::Search(SearchState {
            query: "nonexistent".to_string(),
            cursor: 11,
        });
        let result = render_snapshot(&state, 40, 10, true, true, false);
        assert!(!result.is_empty());
    }

    #[test]
    fn render_single_record() {
        let id = Uuid::new_v4();
        let record = make_record(id, "GitHub", "user@github.com");
        let state = ListPanelState::with_records(vec![record]);
        let result = render_snapshot(&state, 50, 10, true, true, false);
        assert!(!result.is_empty());
    }

    #[test]
    fn render_multiple_records() {
        let r1 = make_record(Uuid::new_v4(), "GitHub", "user@github.com");
        let r2 = make_record_with_type(Uuid::new_v4(), "AWS Key", CredentialType::Api);
        let r3 = make_record_with_type(Uuid::new_v4(), "Server", CredentialType::Ssh);
        let state = ListPanelState::with_records(vec![r1, r2, r3]);
        let result = render_snapshot(&state, 50, 15, true, true, false);
        assert!(!result.is_empty());
    }

    #[test]
    fn render_weak_password_badge() {
        let record = make_record_with_weak(Uuid::new_v4(), "WeakPass");
        let state = ListPanelState::with_records(vec![record]);
        let result = render_snapshot(&state, 50, 10, true, true, false);
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
        let result = render_snapshot(&state, 50, 15, true, true, false);
        assert!(!result.is_empty());
    }

    #[test]
    fn render_search_mode_bar() {
        let mut state = ListPanelState::default();
        state.mode = ListMode::Search(SearchState {
            query: "git".to_string(),
            cursor: 3,
        });
        let result = render_snapshot(&state, 40, 10, true, true, false);
        assert!(!result.is_empty());
    }

    #[test]
    fn render_unfocused() {
        let record = make_record(Uuid::new_v4(), "Test", "subtitle");
        let state = ListPanelState::with_records(vec![record]);
        let result = render_snapshot(&state, 50, 10, false, true, false);
        assert!(!result.is_empty());
    }

    #[test]
    fn render_ascii_mode() {
        let record = make_record(Uuid::new_v4(), "Test", "subtitle");
        let state = ListPanelState::with_records(vec![record]);
        let result = render_snapshot(&state, 50, 10, true, false, false);
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
                ListPanel::view(frame, frame.area(), &state, true, true, false);
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

    // ── Record item building tests ──

    #[test]
    fn build_record_item_login_type() {
        let record = make_record(Uuid::new_v4(), "MyLogin", "user@site.com");
        let item = build_record_item(&record, false, false, true, true, 50);
        assert!(item.height() >= 3); // title + subtitle + separator
    }

    #[test]
    fn build_record_item_api_type() {
        let record = make_record_with_type(Uuid::new_v4(), "AWS", CredentialType::Api);
        let item = build_record_item(&record, false, false, true, true, 50);
        assert!(item.height() >= 3);
    }

    #[test]
    fn build_record_item_ssh_type() {
        let record = make_record_with_type(Uuid::new_v4(), "Server", CredentialType::Ssh);
        let item = build_record_item(&record, false, false, true, true, 50);
        assert!(item.height() >= 3);
    }

    #[test]
    fn build_record_item_selected_indicator() {
        let record = make_record(Uuid::new_v4(), "Test", "sub");
        // With unicode and selected=true, should have ◀
        let item = build_record_item(&record, true, false, true, true, 50);
        assert!(item.height() >= 3);

        // With ASCII and selected=true, should have <
        let item = build_record_item(&record, true, false, true, false, 50);
        assert!(item.height() >= 3);
    }

    #[test]
    fn build_record_item_visual_selected() {
        let record = make_record(Uuid::new_v4(), "Test", "sub");
        let item = build_record_item(&record, false, true, true, true, 50);
        assert!(item.height() >= 3);
    }
}
