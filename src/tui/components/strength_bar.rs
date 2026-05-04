//! Password strength bar widget for U6/U7.

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::crypto::strength::PasswordStrength;
use crate::t;
use crate::tui::theme;

/// Render the password strength bar as a Line.
/// Total width: 16 chars for bar + label text.
pub fn render_strength_bar(strength: &PasswordStrength, unicode: bool) -> Line<'static> {
    let fill = strength.bar_fill as usize;
    let empty = 16 - fill;
    let color = parse_color_hex(strength.level.color_hex());

    let fill_char = if unicode {
        theme::ICON_PROGRESS_FILL
    } else {
        theme::ascii::ICON_PROGRESS_FILL
    };
    let empty_char = if unicode {
        theme::ICON_PROGRESS_EMPTY
    } else {
        theme::ascii::ICON_PROGRESS_EMPTY
    };

    Line::from(vec![
        Span::styled(
            t!("tui.component_labels.strength").to_string(),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled(fill_char.repeat(fill), Style::default().fg(color)),
        Span::styled(empty_char.repeat(empty), Style::default().fg(theme::BORDER)),
        Span::raw(" "),
        Span::styled(
            strength.level.label_zh().to_string(),
            Style::default().fg(color),
        ),
    ])
}

/// Render empty state strength bar (no password entered).
pub fn render_empty_strength_bar() -> Line<'static> {
    Line::from(vec![
        Span::styled(
            t!("tui.component_labels.strength").to_string(),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled(
            t!("tui.component_labels.strength_hint").to_string(),
            Style::default().fg(theme::TEXT_MUTED),
        ),
    ])
}

/// Parse a hex color string like "#f7768e" into a ratatui Color.
fn parse_color_hex(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        Color::Rgb(r, g, b)
    } else {
        Color::White
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::strength::StrengthLevel;

    #[test]
    fn parse_color_hex_valid() {
        match parse_color_hex("#f7768e") {
            Color::Rgb(r, g, b) => {
                assert_eq!(r, 0xf7);
                assert_eq!(g, 0x76);
                assert_eq!(b, 0x8e);
            }
            _ => panic!("Expected Rgb color"),
        }
    }

    #[test]
    fn render_bar_has_correct_fill() {
        let strength = PasswordStrength {
            level: StrengthLevel::Strong,
            char_types: 4,
            bar_fill: 12,
        };
        let line = render_strength_bar(&strength, true);
        assert_eq!(line.spans.len(), 5);
    }

    #[test]
    fn empty_bar_has_hint_text() {
        let line = render_empty_strength_bar();
        assert_eq!(line.spans.len(), 2);
    }
}
