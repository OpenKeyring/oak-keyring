//! Three-panel responsive layout calculation for the main screen.
//!
//! Splits the available terminal area into sidebar, list, and detail panels
//! with a horizontal separator and status bar at the bottom.

use ratatui::layout::Rect;

use crate::tui::terminal::{sidebar_width, WidthTier};

/// Vertical panel separator character.
pub const PANEL_SEPARATOR: &str = "\u{2502}"; // │

/// Horizontal separator line character.
pub const HORIZONTAL_SEPARATOR: &str = "\u{2500}"; // ─

/// Computed layout areas for the main screen.
pub struct MainLayoutAreas {
    /// Sidebar panel (categories/tags navigation).
    pub sidebar: Rect,
    /// Password list panel.
    pub list: Rect,
    /// Password detail panel (remaining space).
    pub detail: Rect,
    /// Horizontal separator between content and status bar (1 row).
    pub status_separator: Rect,
    /// Status bar at the bottom (1 row).
    pub status_bar: Rect,
}

/// Calculate the three-panel layout based on terminal area and width tier.
///
/// Layout structure (top to bottom):
/// - Main content: sidebar (fixed width) | list (30%) | detail (remaining)
/// - Horizontal separator: 1 row
/// - Status bar: 1 row
pub fn calculate_layout(area: Rect, terminal_width: u16) -> MainLayoutAreas {
    let tier = WidthTier::from_width(terminal_width);
    let sw = sidebar_width(tier);

    // Vertical split: reserve 2 rows for separator + status bar
    let main_height = area.height.saturating_sub(2);

    // Horizontal split within main content area
    let list_width = ((area.width - sw) as u32 * 30 / 100) as u16;
    let detail_width = area.width.saturating_sub(sw).saturating_sub(list_width);

    let sidebar = Rect::new(area.x, area.y, sw, main_height);
    let list = Rect::new(area.x + sw, area.y, list_width, main_height);
    let detail = Rect::new(area.x + sw + list_width, area.y, detail_width, main_height);
    let status_separator = Rect::new(area.x, area.y + main_height, area.width, 1);
    let status_bar = Rect::new(area.x, area.y + main_height + 1, area.width, 1);

    MainLayoutAreas {
        sidebar,
        list,
        detail,
        status_separator,
        status_bar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_full_width() {
        let area = Rect::new(0, 0, 120, 30);
        let layout = calculate_layout(area, 120);

        // Full tier: sidebar width = 50
        assert_eq!(layout.sidebar.width, 50);
        assert_eq!(layout.sidebar.height, 28); // 30 - 2 (separator + status)
        assert_eq!(layout.status_bar.height, 1);
        assert_eq!(layout.status_separator.height, 1);
    }

    #[test]
    fn layout_medium_width() {
        let area = Rect::new(0, 0, 110, 30);
        let layout = calculate_layout(area, 110);

        // Medium tier: sidebar width = 40
        assert_eq!(layout.sidebar.width, 40);
    }

    #[test]
    fn layout_minimum_width() {
        let area = Rect::new(0, 0, 80, 30);
        let layout = calculate_layout(area, 80);

        // Minimum tier: sidebar width = 30
        assert_eq!(layout.sidebar.width, 30);
    }

    #[test]
    fn layout_detail_takes_remaining_space() {
        let area = Rect::new(0, 0, 120, 30);
        let layout = calculate_layout(area, 120);

        // All horizontal widths should sum to the total area width
        let total_width = layout.sidebar.width + layout.list.width + layout.detail.width;
        assert_eq!(total_width, area.width);
    }

    #[test]
    fn layout_total_area_conservation() {
        let area = Rect::new(0, 0, 120, 30);
        let layout = calculate_layout(area, 120);

        // Widths sum correctly
        let total_width = layout.sidebar.width + layout.list.width + layout.detail.width;
        assert_eq!(total_width, area.width);

        // Heights account for status bar: main panels + separator + status = total height
        assert_eq!(
            layout.sidebar.height + layout.status_separator.height + layout.status_bar.height,
            area.height
        );
    }
}
