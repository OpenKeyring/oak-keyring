//! Form field inline validation feedback per U11 spec.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::theme;

#[derive(Debug, Clone)]
pub enum ValidationKind {
    Valid(String),
    Invalid(String),
    Warning(String),
}

pub struct InlineValidation;

impl InlineValidation {
    /// Render validation message below a form field.
    pub fn view(frame: &mut Frame, area: Rect, kind: &ValidationKind) {
        let (icon, text, style) = match kind {
            ValidationKind::Valid(msg) => (
                "\u{2713}",
                msg.as_str(),
                Style::default().fg(theme::SUCCESS),
            ),
            ValidationKind::Invalid(msg) => (
                "\u{2715}",
                msg.as_str(),
                Style::default().fg(theme::ERROR),
            ),
            ValidationKind::Warning(msg) => (
                "\u{26A0}",
                msg.as_str(),
                Style::default().fg(theme::WARNING),
            ),
        };
        let line = Line::from(vec![
            Span::styled(
                format!(" {} ", icon),
                style.add_modifier(Modifier::BOLD),
            ),
            Span::styled(text.to_string(), style),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }
}
