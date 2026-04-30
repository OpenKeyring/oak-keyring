//! Import progress bar widget per U11 spec.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::state::loading::ProgressBarState;
use crate::tui::theme;

pub struct ProgressBarWidget;

impl ProgressBarWidget {
    pub fn view(frame: &mut Frame, area: Rect, state: &ProgressBarState, unicode: bool) {
        let filled = (state.width as f64 * state.progress()) as usize;
        let empty = state.width.saturating_sub(filled);

        let (fill_char, empty_char) = if unicode {
            (theme::ICON_PROGRESS_FILL, theme::ICON_PROGRESS_EMPTY)
        } else {
            (
                theme::ascii::ICON_PROGRESS_FILL,
                theme::ascii::ICON_PROGRESS_EMPTY,
            )
        };

        let bar = format!(
            "{}{} {}/{} ({}%) {}",
            fill_char.repeat(filled),
            empty_char.repeat(empty),
            state.current,
            state.total,
            state.percentage(),
            state.label,
        );
        let lines = vec![
            Line::from(Span::styled(bar, Style::default().fg(theme::PRIMARY))),
            Line::from(Span::styled(
                "Press Esc to cancel",
                Style::default().fg(theme::TEXT_MUTED),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }
}
