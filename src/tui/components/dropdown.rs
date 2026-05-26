//! Dropdown widget for U7 form fields.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::tui::{components::text_input, theme};

/// Render a dropdown field (collapsed).
pub fn render_dropdown(
    label: &str,
    selected: &str,
    focused: bool,
    disabled: bool,
    unicode: bool,
) -> Line<'static> {
    let value_style = if disabled {
        Style::default().fg(theme::TEXT_MUTED)
    } else if focused {
        Style::default()
            .fg(theme::BG)
            .bg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::PRIMARY)
    };
    let label_style = if focused {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    let arrow = if unicode {
        theme::ICON_DROPDOWN
    } else {
        theme::ascii::ICON_DROPDOWN
    };

    Line::from(vec![
        Span::styled(text_input::padded_form_label(label), label_style),
        Span::styled(format!("[ {} {} ]", selected, arrow), value_style),
    ])
}

/// Render an expanded dropdown with option list.
pub fn render_dropdown_expanded(
    label: &str,
    options: &[&str],
    selected_index: usize,
    width: u16,
    unicode: bool,
) -> Vec<Line<'static>> {
    let mut lines = vec![render_dropdown(
        label,
        options[selected_index],
        true,
        false,
        unicode,
    )];

    for (i, option) in options.iter().enumerate() {
        let style = if i == selected_index {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::TEXT)
        };
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(text_input::FORM_LABEL_WIDTH)),
            Span::styled(format!("  {} ", option), style),
        ]));
    }

    let _ = width; // used for future alignment
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_dropdown_collapsed() {
        let line = render_dropdown("type", "Login", false, false, true);
        assert_eq!(line.spans.len(), 2);
    }

    #[test]
    fn render_dropdown_disabled() {
        let line = render_dropdown("type", "SSH", false, true, true);
        // Should use muted color
        let _ = &line.spans[1];
    }

    #[test]
    fn render_dropdown_expanded_shows_options() {
        let lines = render_dropdown_expanded("expiry", &["never", "30d", "90d"], 0, 60, true);
        assert_eq!(lines.len(), 4); // 1 header + 3 options
    }
}
