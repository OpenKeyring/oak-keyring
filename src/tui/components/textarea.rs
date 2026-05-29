//! Textarea widget wrapping `tui_textarea::TextArea` with project theme.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Block;
use tui_textarea::TextArea;

use crate::tui::theme;

/// Visible content lines inside the textarea box.
pub const TEXTAREA_VISIBLE_LINES: u16 = 2;

/// Total rows occupied by the textarea widget (top border + content + bottom border).
pub const TEXTAREA_TOTAL_ROWS: u16 = TEXTAREA_VISIBLE_LINES + 2; // ┌─┐ + content + └─┘

/// Create a `TextArea` configured with the project's Tokyo Night theme.
pub fn create_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();

    ta.set_style(
        Style::default()
            .fg(theme::TEXT)
            .bg(theme::BG_SURFACE),
    );
    ta.set_cursor_style(
        Style::default()
            .fg(theme::BG)
            .bg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD),
    );
    ta.set_cursor_line_style(Style::default().bg(theme::BG_SURFACE));
    ta.set_selection_style(Style::default().bg(theme::NL_SELECTED));

    ta
}

/// Configure the textarea block (border) for focused or unfocused state.
pub fn set_block(textarea: &mut TextArea<'static>, focused: bool) {
    let border_color = if focused {
        theme::PRIMARY
    } else {
        theme::NL_LINE
    };
    let block = Block::bordered().border_style(Style::default().fg(border_color));
    textarea.set_block(block);
}

/// Render a labeled textarea field, returning lines for the form layout.
///
/// Returns:
/// - Line 0: label + required/optional marker (same layout as `render_text_input`)
/// - Lines 1..N: placeholder lines for the textarea box area
///
/// Note: The actual `TextArea` widget must be rendered separately via
/// `frame.render_widget(&textarea, area)` because `TextArea` implements
/// `Widget` for `&TextArea`, not for `TextArea` directly.
/// This function returns placeholder lines for row-counting in the
/// line-based form layout; the caller replaces them during actual rendering.
pub fn render_textarea_label(
    label: &str,
    focused: bool,
    is_required: bool,
    _width: u16,
) -> Vec<Line<'static>> {
    let label_style = if focused {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    let marker = if is_required {
        Span::styled(
            crate::t!("tui.component_labels.required").to_string(),
            Style::default().fg(theme::ERROR),
        )
    } else {
        Span::styled(
            crate::t!("tui.component_labels.optional").to_string(),
            Style::default().fg(theme::TEXT_MUTED),
        )
    };

    // Label line with a simple placeholder for the textarea area
    let label_line = Line::from(vec![
        Span::styled(
            super::text_input::padded_form_label(label),
            label_style,
        ),
        marker,
    ]);

    let mut lines = vec![label_line];
    // Placeholder rows for the textarea box (will be overwritten by actual rendering)
    for _ in 0..TEXTAREA_TOTAL_ROWS {
        lines.push(Line::raw(""));
    }
    lines
}

/// Extract the full text content from a `TextArea` as a single `String`.
/// Lines are joined with `\n`.
pub fn textarea_text(textarea: &TextArea<'_>) -> String {
    textarea.lines().join("\n")
}

/// Load text into a `TextArea`, replacing existing content.
pub fn set_textarea_text(textarea: &mut TextArea<'_>, text: &str) {
    textarea.select_all();
    textarea.delete_char();
    if !text.is_empty() {
        textarea.insert_str(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_textarea_applies_theme() {
        let ta = create_textarea();
        // TextArea::default() starts with one empty line
        assert_eq!(ta.lines().len(), 1);
    }

    #[test]
    fn textarea_text_extracts_content() {
        let mut ta = create_textarea();
        ta.insert_str("hello\nworld");
        assert_eq!(textarea_text(&ta), "hello\nworld");
    }

    #[test]
    fn set_textarea_text_replaces_content() {
        let mut ta = create_textarea();
        ta.insert_str("old");
        set_textarea_text(&mut ta, "new\ncontent");
        assert_eq!(textarea_text(&ta), "new\ncontent");
    }

    #[test]
    fn set_textarea_text_handles_empty() {
        let mut ta = create_textarea();
        ta.insert_str("existing");
        set_textarea_text(&mut ta, "");
        assert_eq!(textarea_text(&ta), "");
    }
}
