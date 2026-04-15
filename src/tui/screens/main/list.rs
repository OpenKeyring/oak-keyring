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

use crate::commands::types::{HealthIssue, RecordFilter, SortDirection, SortField};
use crate::tui::components::empty_state::{EmptyStateVariant, EmptyStateWidget};
use crate::tui::state::list_state::{
    format_relative_time, format_type_prefix, ListMode, ListPanelState,
};
use crate::tui::theme;

/// Panel responsible for rendering the password list.
pub struct ListPanel;

impl ListPanel {
    /// Highlight matching portions of `text` that match `query` (case-insensitive).
    ///
    /// Returns a vector of `Span`s where matching substrings are rendered in
    /// yellow bold (`theme::WARNING` + `Modifier::BOLD`) and non-matching
    /// portions in the default text color.
    fn highlight_match(text: &str, query: &str) -> Vec<Span<'static>> {
        if query.is_empty() {
            return vec![Span::styled(text.to_string(), Style::default().fg(theme::TEXT))];
        }
        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();
        let mut spans = Vec::new();
        let mut last_end = 0;

        while let Some(pos) = text_lower[last_end..].find(&query_lower) {
            let abs_pos = last_end + pos;
            if abs_pos > last_end {
                spans.push(Span::styled(
                    text[last_end..abs_pos].to_string(),
                    Style::default().fg(theme::TEXT),
                ));
            }
            spans.push(Span::styled(
                text[abs_pos..abs_pos + query.len()].to_string(),
                Style::default()
                    .fg(theme::WARNING)
                    .add_modifier(Modifier::BOLD),
            ));
            last_end = abs_pos + query.len();
        }
        if last_end < text.len() {
            spans.push(Span::styled(
                text[last_end..].to_string(),
                Style::default().fg(theme::TEXT),
            ));
        }
        spans
    }

    /// Render the list panel.
    ///
    /// # Arguments
    /// * `frame` - The ratatui frame to render into.
    /// * `area` - The rectangular area allocated to the list panel.
    /// * `state` - The current list panel state (records, selection, mode, sort).
    /// * `focused` - Whether the list panel currently has keyboard focus.
    /// * `unicode` - Whether to use unicode characters (vs ASCII fallbacks).
    /// * `filter` - The current record filter, used to select the empty state variant.
    pub fn view(
        frame: &mut Frame,
        area: Rect,
        state: &ListPanelState,
        focused: bool,
        unicode: bool,
        filter: RecordFilter,
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
            render_empty_state(frame, list_area, state, unicode, &filter);
        } else {
            render_list(frame, list_area, state, focused, unicode);
        }
    }
}

// ---------------------------------------------------------------------------
// Health badge
// ---------------------------------------------------------------------------

/// Build a styled health badge span for a given `HealthIssue`.
///
/// Returns `None` for `Expired` (shown only in the detail panel) or when
/// `issue` is `None`.
fn health_badge(issue: Option<&HealthIssue>, unicode: bool) -> Option<Span<'static>> {
    issue.and_then(|i| match i {
        HealthIssue::Compromised => {
            let icon = if unicode { "\u{1F534}" } else { "!" }; // 🔴 / !
            Some(Span::styled(
                format!(" {}\u{5DF2}\u{6CC4}\u{9732}", icon), // " 🔴已泄露"
                Style::default().fg(theme::ERROR),
            ))
        }
        HealthIssue::Weak => {
            let icon = if unicode { "\u{26A0}" } else { "!" }; // ⚠ / !
            Some(Span::styled(
                format!(" {}\u{5F31}", icon), // " ⚠弱"
                Style::default().fg(theme::WARNING),
            ))
        }
        HealthIssue::Duplicate { group_size } => {
            let icon = if unicode { "\u{26A0}" } else { "!" }; // ⚠ / !
            Some(Span::styled(
                format!(" {}\u{91CD}\u{590D}({})", icon, group_size), // " ⚠重复(N)"
                Style::default().fg(theme::WARNING),
            ))
        }
        HealthIssue::Expired => None, // Shown in detail panel, not list badge
    })
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

/// Build visual mode bar: `  多选模式` in BRAND bold + `(N 已选)` in TEXT color
fn build_visual_bar<'a>(selected_count: usize) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            "  \u{591A}\u{9009}\u{6A21}\u{5F0F} ", // "  多选模式 "
            Style::default()
                .fg(theme::BRAND)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "({} \u{5DF2}\u{9009})", // "(N 已选)"
                selected_count
            ),
            Style::default().fg(theme::TEXT),
        ),
    ])
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
    let search_query: Option<&str> = match &state.mode {
        ListMode::Search(s) => Some(&s.query),
        _ => None,
    };

    let items: Vec<ListItem<'_>> = state
        .records
        .iter()
        .enumerate()
        .map(|(idx, record)| {
            let is_selected = state.selected_index == Some(idx);
            let is_visual_selected = visual_ids
                .is_some_and(|ids| ids.contains(&record.id));
            build_record_item(record, is_selected, is_visual_selected, focused, unicode, area.width, search_query)
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
    search_query: Option<&str>,
) -> ListItem<'a> {
    let indicator = if unicode { " \u{25C0}" } else { " <" }; // ◀ / <

    // ── Line 1: Title ──
    let type_prefix = format_type_prefix(&record.credential_type);
    let timestamp = format_relative_time(&record.updated_at);

    // Build name spans: prefix (plain) + highlighted name (if search active)
    let prefix_str = format!("  {}", type_prefix);
    let badge = if record.has_weak_password {
        health_badge(Some(&HealthIssue::Weak), unicode)
    } else {
        None
    };
    let badge_str = badge.as_ref().map(|s| s.content.as_ref()).unwrap_or("");
    let right_part = format!("{}{}", timestamp, if is_selected { indicator } else { "" });

    // Calculate total name content length for padding
    let name_len = prefix_str.chars().count() + record.name.chars().count();

    let padding_len = (area_width as usize)
        .saturating_sub(name_len)
        .saturating_sub(badge_str.chars().count())
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

    // Determine badge span style (override for visual-selected context)
    let badge_span = badge.map(|span| {
        if is_visual_selected {
            Span::styled(
                span.content,
                Style::default()
                    .bg(theme::BRAND)
                    .fg(theme::WARNING)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            span
        }
    });

    // Build title spans with optional search highlighting
    let mut title_spans = vec![Span::styled(prefix_str, base_style)];
    if let Some(query) = search_query {
        title_spans.extend(ListPanel::highlight_match(&record.name, query));
    } else {
        title_spans.push(Span::styled(record.name.clone(), base_style));
    }
    if let Some(badge_s) = badge_span {
        title_spans.push(badge_s);
    }
    title_spans.push(Span::styled(" ".repeat(padding_len), base_style));
    title_spans.push(Span::styled(right_part, base_style));

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

    // Build subtitle with optional search highlighting
    let subtitle_prefix = "  ";
    let subtitle_line = if let Some(query) = search_query {
        let mut sub_spans = vec![Span::styled(subtitle_prefix, subtitle_style)];
        sub_spans.extend(ListPanel::highlight_match(&record.subtitle, query));
        Line::from(sub_spans)
    } else {
        Line::from(Span::styled(
            format!("  {}", record.subtitle),
            subtitle_style,
        ))
    };

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
    filter: &RecordFilter,
) {
    let variant = build_empty_state_variant(state, filter);
    EmptyStateWidget::view(frame, area, &variant, unicode);
}

/// Build the appropriate empty state variant based on list mode and filter.
fn build_empty_state_variant(
    state: &ListPanelState,
    filter: &RecordFilter,
) -> EmptyStateVariant {
    match &state.mode {
        ListMode::Search(search_state) if !search_state.query.is_empty() => {
            EmptyStateVariant::NoSearchResults {
                query: search_state.query.clone(),
            }
        }
        _ => match filter {
            RecordFilter::All => EmptyStateVariant::NoPasswords,
            RecordFilter::Favorites => EmptyStateVariant::NoFavorites,
            RecordFilter::Expired => EmptyStateVariant::NoExpired,
            RecordFilter::HealthIssues => EmptyStateVariant::NoHealthIssues,
            RecordFilter::Trash => EmptyStateVariant::EmptyTrash,
            RecordFilter::Tag(name) => EmptyStateVariant::EmptyTag { tag_name: name.clone() },
            RecordFilter::Search(q) => EmptyStateVariant::NoSearchResults { query: q.clone() },
        },
    }
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
        filter: RecordFilter,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                ListPanel::view(frame, frame.area(), state, focused, unicode, filter);
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
                ListPanel::view(frame, frame.area(), &state, true, true, RecordFilter::All);
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
        // Verify the visual mode bar shows "多选模式" in BRAND bold and selected count in TEXT
        let line = build_visual_bar(5);
        assert_eq!(line.spans.len(), 2, "visual bar should have two spans: label + count");

        // First span: "  多选模式 " in BRAND bold
        let label_span = &line.spans[0];
        assert!(
            label_span.content.as_ref().contains("\u{591A}\u{9009}\u{6A21}\u{5F0F}"),
            "label span should contain '多选模式'"
        );
        assert!(
            label_span.style.fg == Some(theme::BRAND.into()),
            "label should use BRAND color"
        );
        assert!(
            label_span.style.add_modifier.contains(Modifier::BOLD),
            "label should be BOLD"
        );

        // Second span: "(5 已选)" in TEXT color
        let count_span = &line.spans[1];
        assert!(
            count_span.content.as_ref().contains("5"),
            "count span should contain the number 5"
        );
        assert!(
            count_span.content.as_ref().contains("\u{5DF2}\u{9009}"),
            "count span should contain '已选'"
        );
        assert!(
            count_span.style.fg == Some(theme::TEXT.into()),
            "count should use TEXT color"
        );
        assert!(
            !count_span.style.add_modifier.contains(Modifier::BOLD),
            "count should NOT be BOLD"
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
        assert_eq!(spans[1].content.as_ref(), "_");    // normal
        assert_eq!(spans[2].content.as_ref(), "test"); // highlighted
        assert_eq!(spans[3].content.as_ref(), "_");    // normal
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
}
