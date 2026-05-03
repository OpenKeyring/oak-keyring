//! Global notification overlay bar — rendered at top of terminal.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::state::notification::{MessageStyle, StatusMessage};
use crate::tui::theme;

/// Render the current notification message as a centered floating bar at the top.
pub fn render_notification(frame: &mut Frame, area: Rect, msg: &StatusMessage) {
    let (icon, fg_color) = style_for(msg.style);

    let text = format!("{} {}", icon, msg.text);
    let bar_width = (text.len() as u16 + 4).min(area.width);
    let x = (area.width.saturating_sub(bar_width)) / 2;

    let bar_area = Rect {
        x: area.x + x,
        y: area.y,
        width: bar_width,
        height: 1,
    };

    let paragraph = Paragraph::new(text)
        .style(Style::default().fg(fg_color).bg(theme::BG))
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, bar_area);
}

fn style_for(style: MessageStyle) -> (&'static str, Color) {
    match style {
        MessageStyle::Success => (theme::ICON_SUCCESS, theme::SUCCESS),
        MessageStyle::Error => (theme::ICON_ERROR, theme::ERROR),
        MessageStyle::Warning => (theme::ICON_WARNING, theme::WARNING),
        MessageStyle::Operation => (theme::ICON_INFO, theme::INFO),
    }
}
