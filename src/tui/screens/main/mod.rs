pub mod detail;
pub mod layout;
pub mod list;
pub mod overlay;
pub mod sidebar;
pub mod status_bar;

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::commands::types::PanelId;
use crate::tui::screens::main::layout::{calculate_layout, HORIZONTAL_SEPARATOR, PANEL_SEPARATOR};
use crate::tui::screens::main::sidebar::SidebarPanel;
use crate::tui::screens::main::status_bar::StatusBarPanel;
use crate::tui::state::main_state::MainScreenState;
use crate::tui::theme;

/// Main three-panel screen: sidebar | list | detail, with a status bar.
pub struct MainScreen {
    #[allow(dead_code)]
    sidebar: SidebarPanel,
    #[allow(dead_code)]
    list: list::ListPanel,
    #[allow(dead_code)]
    detail: detail::DetailPanel,
    #[allow(dead_code)]
    status_bar: StatusBarPanel,
}

impl Default for MainScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl MainScreen {
    /// Create a new MainScreen with default sub-panels.
    pub fn new() -> Self {
        Self {
            sidebar: SidebarPanel,
            list: list::ListPanel,
            detail: detail::DetailPanel,
            status_bar: StatusBarPanel,
        }
    }

    /// Render the full main screen layout.
    ///
    /// # Arguments
    /// * `frame` - The ratatui frame to render into.
    /// * `area` - The total area available for the main screen.
    /// * `state` - The current main screen state (sidebar, status bar, etc.).
    /// * `focused_panel` - Which panel currently has keyboard focus.
    /// * `unicode` - Whether to use unicode characters (vs ASCII fallbacks).
    pub fn view(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &MainScreenState,
        focused_panel: PanelId,
        unicode: bool,
    ) {
        let terminal_width = frame.area().width;
        let areas = calculate_layout(area, terminal_width);

        // 1. Sidebar
        let sidebar_focused = focused_panel == PanelId::Sidebar;
        SidebarPanel::view(
            frame,
            areas.sidebar,
            &state.sidebar,
            sidebar_focused,
            unicode,
        );

        // 2. List panel
        let list_focused = focused_panel == PanelId::List;
        list::ListPanel::view(
            frame,
            areas.list,
            &state.list,
            list_focused,
            unicode,
            state.current_filter.clone(),
        );

        // 3. Detail panel
        let detail_focused = focused_panel == PanelId::Detail;
        self.detail.view(
            frame,
            areas.detail,
            &state.detail,
            detail_focused,
            unicode,
        );

        // 4. Horizontal separator between content and status bar
        render_horizontal_separator(frame, areas.status_separator, unicode);

        // 5. Vertical separators between panels (only in unicode mode)
        if unicode {
            render_vertical_separators(frame, &areas);
        }

        // 6. Status bar
        StatusBarPanel::view(
            frame,
            areas.status_bar,
            &state.status_bar,
            focused_panel,
            unicode,
        );
    }

    /// Advance focus to next panel: Sidebar -> List -> Detail -> Sidebar.
    pub fn cycle_focus(&self, current: PanelId) -> PanelId {
        match current {
            PanelId::Sidebar => PanelId::List,
            PanelId::List => PanelId::Detail,
            PanelId::Detail => PanelId::Sidebar,
        }
    }

    /// Move focus to previous panel: Sidebar -> Detail -> List -> Sidebar.
    pub fn cycle_focus_reverse(&self, current: PanelId) -> PanelId {
        match current {
            PanelId::Sidebar => PanelId::Detail,
            PanelId::List => PanelId::Sidebar,
            PanelId::Detail => PanelId::List,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_cycle_forward() {
        let screen = MainScreen::new();
        assert_eq!(screen.cycle_focus(PanelId::Sidebar), PanelId::List);
        assert_eq!(screen.cycle_focus(PanelId::List), PanelId::Detail);
        assert_eq!(screen.cycle_focus(PanelId::Detail), PanelId::Sidebar);
    }

    #[test]
    fn focus_cycle_reverse() {
        let screen = MainScreen::new();
        assert_eq!(
            screen.cycle_focus_reverse(PanelId::Sidebar),
            PanelId::Detail
        );
        assert_eq!(screen.cycle_focus_reverse(PanelId::Detail), PanelId::List);
        assert_eq!(screen.cycle_focus_reverse(PanelId::List), PanelId::Sidebar);
    }
}

/// Render the horizontal separator line between content panels and the status bar.
fn render_horizontal_separator(frame: &mut Frame, area: Rect, unicode: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let sep_char = if unicode { HORIZONTAL_SEPARATOR } else { "-" };
    let line: String =
        std::iter::repeat_n(sep_char.chars().next().unwrap_or('-'), area.width as usize).collect();

    let paragraph = Paragraph::new(Line::from(Span::styled(
        line,
        Style::default().fg(theme::BORDER),
    )));
    frame.render_widget(paragraph, area);
}

/// Render vertical separator characters ("│") between the three panels.
///
/// Draws separator lines at the boundaries between sidebar|list and list|detail.
fn render_vertical_separators(frame: &mut Frame, areas: &layout::MainLayoutAreas) {
    let sep_style = Style::default().fg(theme::BORDER);

    // Separator between sidebar and list
    if areas.sidebar.width > 0 && areas.list.width > 0 {
        let x = areas.sidebar.x + areas.sidebar.width;
        // Only render if there is no overlap (the separator column was not
        // allocated to any panel — it visually sits on the border).
        // We render into a 1-column-wide strip at the panel boundary.
        let sep_rect = Rect::new(
            x.saturating_sub(1),
            areas.sidebar.y,
            1,
            areas.sidebar.height,
        );
        let line: String = std::iter::repeat_n(
            PANEL_SEPARATOR.chars().next().unwrap(),
            sep_rect.height as usize,
        )
        .collect();
        let paragraph = Paragraph::new(Line::from(Span::styled(line, sep_style)));
        frame.render_widget(paragraph, sep_rect);
    }

    // Separator between list and detail
    if areas.list.width > 0 && areas.detail.width > 0 {
        let x = areas.list.x + areas.list.width;
        let sep_rect = Rect::new(x.saturating_sub(1), areas.list.y, 1, areas.list.height);
        let line: String = std::iter::repeat_n(
            PANEL_SEPARATOR.chars().next().unwrap(),
            sep_rect.height as usize,
        )
        .collect();
        let paragraph = Paragraph::new(Line::from(Span::styled(line, sep_style)));
        frame.render_widget(paragraph, sep_rect);
    }
}
