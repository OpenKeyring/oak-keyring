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
use ratatui::style::Style;
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

        frame.render_widget(Paragraph::new("").style(theme::Styles::newlook_bg()), area);

        // Split: sort/search bar + one padding row + list body.
        let bar_height = 2u16.min(area.height);
        let bar_area = Rect::new(area.x, area.y, area.width, 1);
        let list_area = Rect::new(
            area.x,
            area.y + bar_height,
            area.width,
            area.height.saturating_sub(bar_height),
        );

        // 1. Render the bar
        let bar_content = build_bar_content(state, unicode);
        let bar =
            Paragraph::new(bar_content).style(Style::default().fg(theme::NL_TEXT).bg(theme::NL_BG));
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

/// Lines per list item: minimum-width panels show 2 lines, others show 3.
fn item_height(width: u16) -> u16 {
    if crate::tui::terminal::WidthTier::from_width(width)
        == crate::tui::terminal::WidthTier::Minimum
    {
        2
    } else {
        3
    }
}

/// Render the scrollable record list with a scrollbar.
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
    let total = state.records.len() as u16;
    let ih = item_height(area.width);
    let visible = area.height / ih;

    // Reserve 1 column for scrollbar if content overflows
    let (list_area, sb_area) = if total > visible && area.width > 4 {
        (
            Rect::new(area.x, area.y, area.width - 1, area.height),
            Rect::new(area.x + area.width - 1, area.y, 1, area.height),
        )
    } else {
        (area, Rect::default())
    };

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
                    list_area.width,
                    retention_days,
                )
            } else {
                build_record_item(
                    record,
                    is_selected,
                    is_visual_selected,
                    focused,
                    unicode,
                    list_area.width,
                    search_query,
                )
            }
        })
        .collect();

    let list = List::new(items);

    let mut list_state = ListState::default();
    list_state.select(state.selected_index);

    frame.render_stateful_widget(list, list_area, &mut list_state);

    render_list_scrollbar(frame, sb_area, list_state.offset(), visible, total);
}

fn render_list_scrollbar(frame: &mut Frame, area: Rect, offset: usize, visible: u16, total: u16) {
    if area.width == 0 || area.height == 0 || total <= visible {
        return;
    }

    let max_offset = (total - visible) as usize;
    if max_offset == 0 {
        return;
    }

    let clamped_offset = offset.min(max_offset);
    let thumb_ratio = visible as f32 / total as f32;
    let thumb_height = ((area.height as f32 * thumb_ratio).max(1.0)).ceil() as u16;
    let scroll_ratio = clamped_offset as f32 / max_offset as f32;
    let max_thumb_y = area.height.saturating_sub(thumb_height);
    let thumb_y = (scroll_ratio * max_thumb_y as f32) as u16;

    // Track
    frame.render_widget(
        Paragraph::new("│".repeat(area.height as usize))
            .style(Style::default().fg(theme::NL_LINE).bg(theme::NL_BG)),
        area,
    );

    // Thumb
    let thumb_area = Rect {
        x: area.x,
        y: area.y + thumb_y,
        width: 1,
        height: thumb_height.max(1),
    };
    frame.render_widget(
        Paragraph::new("█".repeat(thumb_area.height as usize))
            .style(Style::default().fg(theme::NL_CYAN).bg(theme::NL_BG)),
        thumb_area,
    );
}
