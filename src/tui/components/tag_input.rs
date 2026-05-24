//! Tag input widget with autocomplete for U7 form.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::t;
use crate::tui::state::form_state::TagAutocompleteState;
use crate::tui::theme;

fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

fn pad_to_width(value: &str, width: usize) -> String {
    let mut padded = value.to_string();
    let current = display_width(&padded);
    if current < width {
        padded.push_str(&" ".repeat(width - current));
    }
    padded
}

fn truncate_to_width(value: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out
}

/// Render the tag input area.
pub fn render_tag_input(
    input_text: &str,
    tags: &[String],
    focused: bool,
    autocomplete: Option<&TagAutocompleteState>,
    _existing_tags: &[String],
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Input row
    let label_style = if focused {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };
    let input_style = if focused {
        Style::default()
            .fg(theme::BG)
            .bg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT).bg(theme::BG_SURFACE)
    };
    let label = t!("tui.component_labels.tag").to_string();
    let content_width = width.saturating_sub(2) as usize;
    let input_width = content_width
        .saturating_sub(display_width(label.as_str()))
        .saturating_sub(2)
        .clamp(1, 20);
    let input_text = if input_text.is_empty() {
        " "
    } else {
        input_text
    };
    let input_text = pad_to_width(&truncate_to_width(input_text, input_width), input_width);

    lines.push(Line::from(vec![
        Span::styled(label, label_style),
        Span::styled(format!("[{}]", input_text), input_style),
    ]));

    // Tag blocks
    if !tags.is_empty() {
        let mut tag_spans: Vec<Span> = vec![Span::raw("       ")];
        for tag in tags {
            tag_spans.push(Span::styled(
                format!("[ {} ", tag),
                Style::default().fg(theme::BRAND),
            ));
            tag_spans.push(Span::styled("×", Style::default().fg(theme::ERROR)));
            tag_spans.push(Span::styled("] ", Style::default().fg(theme::BRAND)));
        }
        lines.push(Line::from(tag_spans));
    }

    // Autocomplete dropdown
    if let Some(ac) = autocomplete {
        if !ac.matches.is_empty() {
            for (i, tag) in ac.matches.iter().enumerate() {
                let style = if i == ac.selected_index {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default().fg(theme::TEXT)
                };
                lines.push(Line::from(vec![
                    Span::raw("       "),
                    Span::styled(format!("  {} ", tag), style),
                ]));
            }
        }
    }

    lines
}

/// Filter existing tags by input prefix.
pub fn filter_tags<'a>(input: &str, existing: &'a [String]) -> Vec<&'a String> {
    if input.is_empty() {
        return Vec::new();
    }
    let lower = input.to_lowercase();
    existing
        .iter()
        .filter(|t| t.to_lowercase().contains(&lower))
        .take(5)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_tag_input_empty() {
        let lines = render_tag_input("", &[], false, None, &[], 60);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn render_tag_input_with_tags() {
        let tags = vec!["工作".into(), "GitHub".into()];
        let lines = render_tag_input("", &tags, false, None, &[], 60);
        assert!(lines.len() >= 2);
    }

    #[test]
    fn filter_tags_finds_match() {
        let existing = vec!["工作".into(), "个人".into(), "购物".into()];
        let matches = filter_tags("工", &existing);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "工作");
    }

    #[test]
    fn filter_tags_empty_input() {
        let existing = vec!["工作".into()];
        let matches = filter_tags("", &existing);
        assert!(matches.is_empty());
    }

    #[test]
    fn filter_tags_no_match() {
        let existing = vec!["工作".into()];
        let matches = filter_tags("xyz", &existing);
        assert!(matches.is_empty());
    }

    #[test]
    fn filter_tags_case_insensitive() {
        let existing = vec!["GitHub".into(), "GitLab".into()];
        let matches = filter_tags("git", &existing);
        assert_eq!(matches.len(), 2);
    }
}
