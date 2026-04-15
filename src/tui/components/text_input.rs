//! Text input widget for U7 form fields.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::tui::theme;

/// Render a labeled text input field.
pub fn render_text_input(
    label: &str,
    value: &str,
    focused: bool,
    has_error: bool,
    is_required: bool,
    is_masked: bool,
    width: u16,
) -> Vec<Line<'static>> {
    let display_value = if is_masked {
        "\u{2022}".repeat(value.chars().count())
    } else {
        value.to_string()
    };

    let _border_color = if has_error {
        theme::ERROR
    } else if focused {
        theme::PRIMARY
    } else {
        theme::BORDER
    };

    let label_style = Style::default().fg(theme::TEXT_SECONDARY);
    let required_mark = if is_required {
        Span::styled(
            " \u{2190} \u{5FC5}\u{586B}",
            Style::default().fg(theme::ERROR),
        )
    } else {
        Span::styled(
            " \u{2190} \u{9009}\u{586B}",
            Style::default().fg(theme::TEXT_MUTED),
        )
    };

    let input_inner_width = width.saturating_sub(label.len() as u16 + 4);
    let padded_value = format!(
        "{:<width$}",
        display_value,
        width = input_inner_width as usize
    );

    vec![Line::from(vec![
        Span::styled(format!("  {} ", label), label_style),
        Span::styled(
            format!("[{}]", padded_value),
            Style::default().fg(theme::TEXT).bg(theme::BG_SURFACE),
        ),
        required_mark,
    ])]
}

/// Render a masked input with action buttons.
pub fn render_password_input_with_buttons(
    label: &str,
    value: &str,
    focused: bool,
    has_error: bool,
    buttons: &[(&str, bool)], // (label, is_focused)
    width: u16,
) -> Vec<Line<'static>> {
    let display_value = "\u{2022}".repeat(value.chars().count());
    let _border_color = if has_error {
        theme::ERROR
    } else if focused {
        theme::PRIMARY
    } else {
        theme::BORDER
    };

    let input_width = width.saturating_sub(label.len() as u16 + buttons.len() as u16 * 12 + 4);
    let padded = format!("{:<width$}", display_value, width = input_width as usize);

    let mut spans = vec![
        Span::styled(
            format!("  {} ", label),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled(
            format!("[{}]", padded),
            Style::default().fg(theme::TEXT).bg(theme::BG_SURFACE),
        ),
    ];

    for (btn_label, btn_focused) in buttons {
        let style = if *btn_focused {
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::PRIMARY)
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("[ {} ]", btn_label), style));
    }

    vec![Line::from(spans)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_text_input_shows_label() {
        let lines = render_text_input("name", "GitHub", false, false, true, false, 60);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn render_text_input_shows_required() {
        let lines = render_text_input("name", "", false, false, true, false, 60);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("\u{5FC5}\u{586B}"));
    }

    #[test]
    fn render_masked_input_shows_dots() {
        let lines = render_text_input("pass", "secret", false, false, true, true, 60);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"));
    }
}
