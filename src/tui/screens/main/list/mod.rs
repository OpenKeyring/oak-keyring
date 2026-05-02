//! List panel rendering for the main screen (U3 spec).
//!
//! Renders:
//! - Sort/search/visual-mode bar at top (1 line)
//! - Two-line list items with type prefix, name, health badge, timestamp, separator
//! - Empty state fallback when no records are present

pub mod bar;
pub mod empty;
pub mod items;
#[cfg(test)]
pub mod tests;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::commands::types::RecordFilter;
use crate::tui::state::list_state::{ListMode, ListPanelState};
use crate::tui::theme;

use self::bar::build_bar_content;
use self::empty::render_empty_state;
use self::items::{build_record_item, build_trash_item};

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
    /// * `filter` - The current record filter, used to select the empty state variant.
    pub fn view(
        frame: &mut Frame,
        area: Rect,
        state: &ListPanelState,
        focused: bool,
        unicode: bool,
        filter: RecordFilter,
        retention_days: u32,
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
            render_list(
                frame,
                list_area,
                state,
                focused,
                unicode,
                &filter,
                retention_days,
            );
        }
    }
}

/// Render the scrollable record list.
fn render_list(
    frame: &mut Frame,
    area: Rect,
    state: &ListPanelState,
    focused: bool,
    unicode: bool,
    filter: &RecordFilter,
    retention_days: u32,
) {
    let visual_ids = match &state.mode {
        ListMode::Visual(vs) => Some(&vs.selected_ids),
        _ => None,
    };
    let search_query: Option<&str> = match &state.mode {
        ListMode::Search(s) => Some(&s.query),
        _ => None,
    };

    let is_trash = matches!(filter, RecordFilter::Trash);

    let items: Vec<ListItem<'_>> = state
        .records
        .iter()
        .enumerate()
        .map(|(idx, record)| {
            let is_selected = state.selected_index == Some(idx);
            let is_visual_selected = visual_ids.is_some_and(|ids| ids.contains(&record.id));
            if is_trash {
                build_trash_item(
                    record,
                    is_selected,
                    is_visual_selected,
                    focused,
                    unicode,
                    area.width,
                    retention_days,
                )
            } else {
                build_record_item(
                    record,
                    is_selected,
                    is_visual_selected,
                    focused,
                    unicode,
                    area.width,
                    search_query,
                )
            }
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
