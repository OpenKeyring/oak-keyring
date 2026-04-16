//! Braille dot spinner widget.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::state::loading::SpinnerState;
use crate::tui::theme;

pub struct SpinnerWidget;

impl SpinnerWidget {
    /// Render a centered spinner with label.
    pub fn view(frame: &mut Frame, area: Rect, state: &SpinnerState, unicode: bool) {
        let frame_char = if unicode {
            state.frame()
        } else {
            let ascii = SpinnerState::frames_ascii();
            ascii[state.frame_index % ascii.len()]
        };
        let text = format!("{} {}", frame_char, state.label);
        let para = Paragraph::new(Span::styled(text, Style::default().fg(theme::TEXT)));
        frame.render_widget(para, area);
    }
}
