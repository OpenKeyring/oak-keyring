//! Textarea widget wrapping `tui_textarea::TextArea` with project theme.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use tui_textarea::{TextArea, WrapMode};
use unicode_width::UnicodeWidthStr;

use crate::tui::theme;

/// Minimum visible content lines inside the textarea box.
pub const TEXTAREA_MIN_VISIBLE_LINES: u16 = 3;

/// Maximum visible content lines inside the textarea box.
pub const TEXTAREA_MAX_VISIBLE_LINES: u16 = 8;

/// Create a `TextArea` configured with the project's Tokyo Night theme.
pub fn create_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();

    ta.set_style(Style::default().fg(theme::TEXT).bg(theme::BG_SURFACE));
    ta.set_cursor_style(
        Style::default()
            .fg(theme::BG)
            .bg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD),
    );
    ta.set_cursor_line_style(Style::default().bg(theme::BG_SURFACE));
    ta.set_selection_style(Style::default().bg(theme::NL_SELECTED));
    ta.set_wrap_mode(WrapMode::WordOrGlyph);

    // No block border — the form's line-based layout draws the frame manually.
    ta.remove_block();

    ta
}

/// Update the textarea block style based on focus state.
/// Since we use no block in the form, this is a no-op kept for API compatibility.
pub fn update_block(textarea: &mut TextArea<'_>, _focused: bool) {
    textarea.remove_block();
}

/// Make the embedded textarea cursor visually disappear for read-only render passes.
pub fn hide_cursor(textarea: &mut TextArea<'_>) {
    textarea.set_cursor_style(Style::default().fg(theme::TEXT).bg(theme::BG_SURFACE));
    textarea.set_cursor_line_style(Style::default().bg(theme::BG_SURFACE));
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
    visible_rows: u16,
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
        Span::styled(super::text_input::padded_form_label(label), label_style),
        marker,
    ]);

    let mut lines = vec![label_line];
    // Placeholder rows for the textarea box (will be overwritten by actual rendering)
    for _ in 0..visible_rows {
        lines.push(Line::raw(""));
    }
    lines
}

/// Return the visible textarea height for the current content.
pub fn visible_rows(textarea: &TextArea<'_>) -> u16 {
    let line_count = textarea.lines().len().max(1) as u16;
    line_count.clamp(TEXTAREA_MIN_VISIBLE_LINES, TEXTAREA_MAX_VISIBLE_LINES)
}

/// Return the visible textarea height, accounting for soft-wrapped long lines.
pub fn visible_rows_for_width(textarea: &TextArea<'_>, content_width: u16) -> u16 {
    let width = content_width.max(1) as usize;
    let rows: usize = textarea
        .lines()
        .iter()
        .map(|line| {
            let display_width = UnicodeWidthStr::width(line.as_str()).max(1);
            display_width.div_ceil(width)
        })
        .sum();
    (rows.max(1) as u16).clamp(TEXTAREA_MIN_VISIBLE_LINES, TEXTAREA_MAX_VISIBLE_LINES)
}

/// Whether the cursor can move to a previous line within the textarea.
pub fn cursor_has_line_above(textarea: &TextArea<'_>) -> bool {
    let (row, _) = textarea.cursor();
    row > 0
}

/// Whether the cursor can move to a following line within the textarea.
pub fn cursor_has_line_below(textarea: &TextArea<'_>) -> bool {
    let (row, _) = textarea.cursor();
    row + 1 < textarea.lines().len()
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
    fn create_textarea_soft_wraps_long_lines() {
        let ta = create_textarea();
        assert_eq!(ta.wrap_mode(), tui_textarea::WrapMode::WordOrGlyph);
    }

    #[test]
    fn visible_rows_for_width_counts_wrapped_lines() {
        let mut ta = create_textarea();
        set_textarea_text(&mut ta, "abcdefghijklmnop");
        assert_eq!(visible_rows_for_width(&ta, 8), 3);
        set_textarea_text(&mut ta, &"a".repeat(80));
        assert_eq!(visible_rows_for_width(&ta, 8), 8);
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

    #[test]
    fn visible_rows_expands_to_content_up_to_eight_lines() {
        let mut ta = create_textarea();
        assert_eq!(visible_rows(&ta), 3);

        set_textarea_text(&mut ta, "one\ntwo\nthree\nfour");
        assert_eq!(visible_rows(&ta), 4);

        set_textarea_text(&mut ta, "1\n2\n3\n4\n5\n6\n7\n8");
        assert_eq!(visible_rows(&ta), 8);

        set_textarea_text(&mut ta, "1\n2\n3\n4\n5\n6\n7\n8\n9");
        assert_eq!(visible_rows(&ta), 8);
    }
}
