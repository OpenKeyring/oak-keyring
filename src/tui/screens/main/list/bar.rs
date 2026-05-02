use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::commands::types::{SortDirection, SortField};
use crate::tui::state::list_state::{ListMode, ListPanelState};
use crate::tui::theme;

/// Build the bar content based on the current list mode.
pub(super) fn build_bar_content<'a>(state: &ListPanelState, unicode: bool) -> Line<'a> {
    match &state.mode {
        ListMode::Normal => build_sort_bar(&state.sort.field, &state.sort.direction, unicode),
        ListMode::Search(search_state) => build_search_bar(&search_state.query, unicode),
        ListMode::Visual(visual_state) => build_visual_bar(visual_state.selected_ids.len()),
    }
}

/// Build sort bar: `  排序: [ 排序字段 ▼ ]  [ ↑/↓ 升序/降序 ]`
pub(super) fn build_sort_bar<'a>(
    field: &SortField,
    direction: &SortDirection,
    unicode: bool,
) -> Line<'a> {
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
pub(super) fn build_search_bar<'a>(query: &str, unicode: bool) -> Line<'a> {
    let search_icon = if unicode { "\u{1F50D}" } else { ">" }; // 🔍 / >
    let display_query = format!("{} \u{641C}\u{7D22}: {}_", search_icon, query); // "🔍 搜索: <query>_"

    Line::from(vec![Span::styled(
        format!("  {}", display_query),
        Style::default().fg(theme::TEXT),
    )])
}

/// Build visual mode bar: `  多选模式` in TEXT bold on BG_BAR + `(N 已选)` in TEXT on BG_BAR
pub(super) fn build_visual_bar<'a>(selected_count: usize) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            "  \u{591A}\u{9009}\u{6A21}\u{5F0F} ", // "  多选模式 "
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD)
                .bg(theme::BG_BAR),
        ),
        Span::styled(
            format!(
                "({} \u{5DF2}\u{9009})", // "(N 已选)"
                selected_count
            ),
            Style::default().fg(theme::TEXT).bg(theme::BG_BAR),
        ),
    ])
}

/// Return the Chinese display label for a sort field.
pub(super) fn sort_field_label(field: &SortField) -> &'static str {
    match field {
        SortField::CreatedAt => "\u{521B}\u{5EFA}\u{65F6}\u{95F4}", // 创建时间
        SortField::UpdatedAt => "\u{66F4}\u{65B0}\u{65F6}\u{95F4}", // 更新时间
        SortField::Name => "\u{540D}\u{79F0}",                      // 名称
        SortField::UsageFrequency => "\u{4F7F}\u{7528}\u{9891}\u{7387}", // 使用频率
    }
}

/// Return the icon and Chinese label for a sort direction.
pub(super) fn sort_direction_label(
    direction: &SortDirection,
    unicode: bool,
) -> (&'static str, &'static str) {
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
