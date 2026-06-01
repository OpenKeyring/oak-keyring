//! Text input widget for U7 form fields.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::t;
use crate::tui::state::form_state::PasswordFieldFocus;
use crate::tui::theme;

pub(crate) const FORM_LABEL_WIDTH: usize = 13;
const FORM_SAFETY_PADDING: usize = 2;

pub(crate) fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

fn char_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0)
}

pub(crate) fn pad_to_width(value: &str, width: usize) -> String {
    let mut padded = value.to_string();
    let current = display_width(&padded);
    if current < width {
        padded.push_str(&" ".repeat(width - current));
    }
    padded
}

fn truncate_to_width(value: &str, width: usize) -> String {
    let total = display_width(value);
    if total <= width {
        return value.to_string();
    }

    // Content exceeds width: show the tail so the cursor (always at end) is visible.
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars().rev() {
        let ch_width = char_width(ch);
        if used + ch_width > width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.chars().rev().collect()
}

fn take_prefix_to_width(value: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let ch_width = char_width(ch);
        if used + ch_width > width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out
}

pub(crate) fn padded_form_label(label: &str) -> String {
    pad_to_width(&format!("  {}", label.trim()), FORM_LABEL_WIDTH)
}

pub(crate) fn render_input_box_spans(
    value: &str,
    width: usize,
    focused: bool,
    style: Style,
) -> Vec<Span<'static>> {
    if !focused {
        let display_value = truncate_to_width(value, width);
        let padded_value = pad_to_width(&display_value, width);
        return vec![Span::styled(format!("[{}]", padded_value), style)];
    }

    let text_width = width.saturating_sub(1);
    let display_value = truncate_to_width(value, text_width);
    let used = display_width(&display_value);
    let rest_width = width.saturating_sub(used + 1);
    let cursor_style = Style::default()
        .fg(theme::BG)
        .bg(theme::PRIMARY)
        .add_modifier(Modifier::BOLD);

    vec![
        Span::styled("[", style),
        Span::styled(display_value, style),
        Span::styled(" ", cursor_style),
        Span::styled(" ".repeat(rest_width), style),
        Span::styled("]", style),
    ]
}

pub(crate) fn render_bare_input_spans_at_cursor(
    value: &str,
    placeholder: &str,
    cursor: usize,
    width: usize,
    focused: bool,
    style: Style,
    placeholder_style: Style,
) -> Vec<Span<'static>> {
    if !focused {
        let display_value = if value.is_empty() {
            truncate_to_width(placeholder, width)
        } else {
            truncate_to_width(value, width)
        };
        let padded_value = pad_to_width(&display_value, width);
        let display_style = if value.is_empty() {
            placeholder_style
        } else {
            style
        };
        return vec![Span::styled(padded_value, display_style)];
    }

    let mut cursor = cursor.min(value.len());
    while cursor > 0 && !value.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let text_width = width.saturating_sub(1);
    let before = truncate_to_width(&value[..cursor], text_width);
    let after_width = text_width.saturating_sub(display_width(&before));
    let after = take_prefix_to_width(&value[cursor..], after_width);
    let used = display_width(&before) + display_width(&after);
    let rest_width = width.saturating_sub(used + 1);
    let cursor_style = Style::default()
        .fg(theme::BG)
        .bg(theme::PRIMARY)
        .add_modifier(Modifier::BOLD);

    vec![
        Span::styled(before, style),
        Span::styled(" ", cursor_style),
        Span::styled(after, style),
        Span::styled(" ".repeat(rest_width), style),
    ]
}

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
        theme::ICON_PASSWORD_MASK.repeat(value.chars().count())
    } else {
        value.to_string()
    };

    let label_style = if focused {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };
    let required_mark = if is_required {
        Span::styled(
            t!("tui.component_labels.required").to_string(),
            Style::default().fg(theme::ERROR),
        )
    } else {
        Span::styled(
            t!("tui.component_labels.optional").to_string(),
            Style::default().fg(theme::TEXT_MUTED),
        )
    };

    let marker_width = display_width(required_mark.content.as_ref());
    let content_width = width.saturating_sub(2) as usize;
    let input_inner_width = content_width
        .saturating_sub(FORM_LABEL_WIDTH)
        .saturating_sub(marker_width)
        .saturating_sub(2)
        .saturating_sub(FORM_SAFETY_PADDING)
        .max(1);
    let input_style = if has_error {
        Style::default().fg(theme::ERROR).bg(theme::BG_SURFACE)
    } else {
        Style::default().fg(theme::TEXT).bg(theme::BG_SURFACE)
    };

    let mut spans = vec![Span::styled(padded_form_label(label), label_style)];
    spans.extend(render_input_box_spans(
        &display_value,
        input_inner_width,
        focused,
        input_style,
    ));
    spans.push(required_mark);
    vec![Line::from(spans)]
}

/// A button descriptor for password input inline buttons.
pub struct PasswordButton {
    pub label: String,
    pub focus_variant: PasswordFieldFocus,
}

/// Render a masked input with action buttons.
///
/// When `visible` is true, the value is shown as plaintext instead of bullets.
/// The button matching `focused_button` (if any) is rendered with reversed highlight.
#[allow(clippy::too_many_arguments)]
pub fn render_password_input_with_buttons(
    label: &str,
    value: &str,
    focused: bool,
    _has_error: bool,
    visible: bool,
    buttons: &[PasswordButton],
    focused_button: Option<PasswordFieldFocus>,
    width: u16,
) -> Vec<Line<'static>> {
    let display_value = if visible {
        value.to_string()
    } else {
        theme::ICON_PASSWORD_MASK.repeat(value.chars().count())
    };
    let button_width: usize = buttons
        .iter()
        .map(|button| display_width(&format!(" [ {} ]", button.label)))
        .sum::<usize>()
        + buttons.len();
    let content_width = width.saturating_sub(2) as usize;
    let input_width = content_width
        .saturating_sub(FORM_LABEL_WIDTH)
        .saturating_sub(button_width)
        .saturating_sub(FORM_SAFETY_PADDING)
        .max(1);
    let input_focused =
        focused && focused_button.is_none_or(|button| button == PasswordFieldFocus::Input);
    let input_style = Style::default().fg(theme::TEXT).bg(theme::BG_SURFACE);
    let label_style = if focused {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    let mut spans = vec![Span::styled(padded_form_label(label), label_style)];
    spans.extend(render_input_box_spans(
        &display_value,
        input_width,
        input_focused,
        input_style,
    ));

    for btn in buttons {
        let is_focused = focused_button == Some(btn.focus_variant);
        let style = if is_focused {
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::PRIMARY)
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("[ {} ]", btn.label), style));
    }

    vec![Line::from(spans)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::i18n::LocaleGuard;

    #[test]
    fn render_text_input_shows_label() {
        let lines = render_text_input("name", "GitHub", false, false, true, false, 60);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn render_text_input_shows_required() {
        let _guard = LocaleGuard::zh_cn();
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

    #[test]
    fn render_password_input_visible_shows_plaintext() {
        let buttons = [
            PasswordButton {
                label: "显示".to_string(),
                focus_variant: PasswordFieldFocus::Show,
            },
            PasswordButton {
                label: "复制".to_string(),
                focus_variant: PasswordFieldFocus::Copy,
            },
        ];
        let lines = render_password_input_with_buttons(
            "密码", "hunter2", false, false, true, &buttons, None, 60,
        );
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("hunter2"));
    }

    #[test]
    fn render_password_input_masked_shows_bullets() {
        let buttons = [PasswordButton {
            label: "显示".to_string(),
            focus_variant: PasswordFieldFocus::Show,
        }];
        let lines = render_password_input_with_buttons(
            "密码", "secret", false, false, false, &buttons, None, 60,
        );
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!text.contains("secret"));
        assert!(text.contains("\u{2022}"));
    }
}
