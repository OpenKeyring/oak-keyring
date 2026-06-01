//! Length slider widget for U6 password generator.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::tui::theme;

pub const SLIDER_BAR_WIDTH: usize = 20;

/// Render a length slider row.
/// Label | \[ value \] | \[-\] bar \[+\]
pub fn render_length_slider(
    label: &str,
    value: usize,
    min: usize,
    max: usize,
    focused: bool,
) -> Line<'static> {
    let fill_ratio = (value - min) as f64 / (max - min) as f64;
    let mut fill_chars = (fill_ratio * SLIDER_BAR_WIDTH as f64).ceil() as usize;
    if value == min {
        fill_chars = 0;
    }
    let fill_chars = fill_chars.min(SLIDER_BAR_WIDTH);
    let empty_chars = SLIDER_BAR_WIDTH - fill_chars;

    let minus_style = if value > min {
        Style::default().fg(theme::PRIMARY)
    } else {
        Style::default().fg(theme::TEXT_MUTED)
    };

    let plus_style = if value < max {
        Style::default().fg(theme::PRIMARY)
    } else {
        Style::default().fg(theme::TEXT_MUTED)
    };

    let value_style = if focused {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(theme::TEXT)
    };

    let bar_fill_style = if focused {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::PRIMARY)
    };

    Line::from(vec![
        Span::styled(
            format!("  {} ", label),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled(format!("[ {} ]", value), value_style),
        Span::raw("  "),
        Span::styled("[-]", minus_style),
        Span::raw(" "),
        Span::styled("█".repeat(fill_chars), bar_fill_style),
        Span::styled("░".repeat(empty_chars), Style::default().fg(theme::BORDER)),
        Span::raw(" "),
        Span::styled("[+]", plus_style),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_slider_mid_range() {
        let line = render_length_slider("长度", 16, 8, 128, false);
        assert_eq!(line.spans.len(), 9);
    }

    #[test]
    fn render_slider_at_min() {
        let line = render_length_slider("长度", 8, 8, 128, false);
        // At min, minus button should be muted - just verify it renders
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn render_slider_at_max() {
        let line = render_length_slider("长度", 128, 8, 128, false);
        // At max, plus button should be muted - just verify it renders
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn render_slider_focused_style() {
        let line = render_length_slider("长度", 16, 8, 128, true);
        // Verify it renders with focused style
        assert!(!line.spans.is_empty());
    }
}
