use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::commands::types::{SortDirection, SortField};
use crate::t;
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

/// Build sort bar: `  <sort>: [ sort field ▼ ]  [ ↑/↓ asc/desc ]`
pub(super) fn build_sort_bar<'a>(
    field: &SortField,
    direction: &SortDirection,
    unicode: bool,
) -> Line<'a> {
    let field_name = sort_field_label(field);
    let (dir_icon, dir_label) = sort_direction_label(direction, unicode);
    let down_icon = if unicode {
        theme::ICON_DROPDOWN
    } else {
        theme::ascii::ICON_DROPDOWN
    };
    let sort_label = t!("tui.password_list.sort_label");

    Line::from(vec![
        Span::styled(
            format!("  {}: [ ", sort_label),
            Style::default().fg(theme::NL_TEXT_MUTED).bg(theme::NL_BG),
        ),
        Span::styled(
            format!("{} {}", field_name, down_icon),
            Style::default().fg(theme::NL_TEXT).bg(theme::NL_BG),
        ),
        Span::styled(
            " ]  [ ",
            Style::default().fg(theme::NL_TEXT_MUTED).bg(theme::NL_BG),
        ),
        Span::styled(
            format!("{} {}", dir_icon, dir_label),
            Style::default().fg(theme::NL_CYAN).bg(theme::NL_BG),
        ),
        Span::styled(
            " ]",
            Style::default().fg(theme::NL_TEXT_MUTED).bg(theme::NL_BG),
        ),
    ])
}

/// Build search bar: `  🔍 <search>: <query>_`
pub(super) fn build_search_bar<'a>(query: &str, unicode: bool) -> Line<'a> {
    let search_icon = if unicode {
        theme::ICON_SEARCH
    } else {
        theme::ascii::ICON_SEARCH
    };
    let search_label = t!("tui.password_list.search_prompt");
    let display_query = format!("{} {}{}_", search_icon, search_label, query);

    Line::from(vec![Span::styled(
        format!("  {}", display_query),
        Style::default().fg(theme::NL_TEXT).bg(theme::NL_BG),
    )])
}

/// Build visual mode bar: `  <visual mode>` in TEXT bold on BG_BAR + `(N selected)` in TEXT on BG_BAR
pub(super) fn build_visual_bar<'a>(selected_count: usize) -> Line<'a> {
    let visual_label = t!("tui.password_list.visual_mode");
    let selected_label = t!("tui.password_list.selected_count", count = selected_count);
    Line::from(vec![
        Span::styled(
            format!("  {} ", visual_label),
            Style::default()
                .fg(theme::NL_TEXT)
                .add_modifier(Modifier::BOLD)
                .bg(theme::NL_SELECTED),
        ),
        Span::styled(
            format!("({})", selected_label),
            Style::default().fg(theme::NL_TEXT).bg(theme::NL_SELECTED),
        ),
    ])
}

/// Return the display label for a sort field.
pub(super) fn sort_field_label(field: &SortField) -> String {
    match field {
        SortField::CreatedAt => t!("tui.password_list.sort_created").to_string(),
        SortField::UpdatedAt => t!("tui.password_list.sort_updated").to_string(),
        SortField::Name => t!("tui.password_list.sort_by_name").to_string(),
        SortField::UsageFrequency => t!("tui.password_list.sort_frequency").to_string(),
    }
}

/// Return the icon and label for a sort direction.
pub(super) fn sort_direction_label(
    direction: &SortDirection,
    unicode: bool,
) -> (&'static str, String) {
    match direction {
        SortDirection::Desc => {
            let label = t!("tui.password_list.sort_direction_desc");
            if unicode {
                ("\u{2193}", label.to_string()) // ↓
            } else {
                ("v", label.to_string())
            }
        }
        SortDirection::Asc => {
            let label = t!("tui.password_list.sort_direction_asc");
            if unicode {
                ("\u{2191}", label.to_string()) // ↑
            } else {
                ("^", label.to_string())
            }
        }
    }
}
