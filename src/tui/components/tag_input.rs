//! Tag input widget with autocomplete for U7 form.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::t;
use crate::tui::state::form_state::TagAutocompleteState;
use crate::tui::theme;

/// Render the tag input area.
pub fn render_tag_input(
    input_text: &str,
    tags: &[String],
    _focused: bool,
    autocomplete: Option<&TagAutocompleteState>,
    _existing_tags: &[String],
    _width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Input row
    lines.push(Line::from(vec![
        Span::styled(
            t!("tui.component_labels.tag").to_string(),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled(
            format!(
                "[{:<20}]",
                if input_text.is_empty() {
                    " "
                } else {
                    input_text
                }
            ),
            Style::default().fg(theme::TEXT).bg(theme::BG_SURFACE),
        ),
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
