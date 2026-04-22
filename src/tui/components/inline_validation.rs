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
    pub fn view(frame: &mut Frame, area: Rect, kind: &ValidationKind, unicode: bool) {
        let (icon, text, style) = match kind {
            ValidationKind::Valid(msg) => (
                if unicode { theme::ICON_SUCCESS } else { theme::ascii::ICON_SUCCESS },
                msg.as_str(),
                Style::default().fg(theme::SUCCESS),
            ),
            ValidationKind::Invalid(msg) => (
                if unicode { theme::ICON_ERROR } else { theme::ascii::ICON_ERROR },
                msg.as_str(),
                Style::default().fg(theme::ERROR),
            ),
            ValidationKind::Warning(msg) => (
                if unicode { theme::ICON_WARNING } else { theme::ascii::ICON_WARNING },
                msg.as_str(),
                Style::default().fg(theme::WARNING),
            ),
        };
        let line = Line::from(vec![
            Span::styled(format!(" {} ", icon), style.add_modifier(Modifier::BOLD)),
            Span::styled(text.to_string(), style),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn cell_content(kind: &ValidationKind, unicode: bool) -> String {
        let backend = TestBackend::new(40, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                InlineValidation::view(f, Rect::new(0, 0, 40, 3), kind, unicode);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        format!("{:?}", buf)
    }

    #[test]
    fn valid_unicode_shows_checkmark() {
        let out = cell_content(&ValidationKind::Valid("ok".into()), true);
        assert!(out.contains('\u{2713}'), "should contain ✓");
    }

    #[test]
    fn valid_ascii_shows_plus() {
        let out = cell_content(&ValidationKind::Valid("ok".into()), false);
        assert!(out.contains('+'), "should contain ASCII +");
        assert!(!out.contains('\u{2713}'), "should not contain ✓");
    }

    #[test]
    fn invalid_unicode_shows_cross() {
        let out = cell_content(&ValidationKind::Invalid("bad".into()), true);
        assert!(out.contains('\u{2715}'), "should contain ✕");
    }

    #[test]
    fn invalid_ascii_shows_x() {
        let out = cell_content(&ValidationKind::Invalid("bad".into()), false);
        assert!(out.contains('x'), "should contain ASCII x");
        assert!(!out.contains('\u{2715}'), "should not contain ✕");
    }

    #[test]
    fn warning_unicode_shows_warn_sign() {
        let out = cell_content(&ValidationKind::Warning("meh".into()), true);
        assert!(out.contains('\u{26A0}'), "should contain ⚠");
    }

    #[test]
    fn warning_ascii_shows_exclamation() {
        let out = cell_content(&ValidationKind::Warning("meh".into()), false);
        assert!(out.contains('!'), "should contain ASCII !");
        assert!(!out.contains('\u{26A0}'), "should not contain ⚠");
    }
}
