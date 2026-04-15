//! Batch tag panel overlay — tag management for multiple selected records.
//!
//! Provides a floating panel with four focus zones:
//! Input → CurrentTags → AvailableTags → DoneButton (cycles).

use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::state::overlay_state::{BatchTagPanelFullState, TagPanelFocus};
use crate::tui::theme;

// ── Action type ──────────────────────────────────────────────────────────────

/// Actions returned by keyboard handling for the caller to dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchTagAction {
    /// No action taken; event consumed internally.
    None,
    /// User confirmed adding a new tag (from input field).
    AddTag(String),
    /// User requested removing a tag from current tags.
    RemoveTag(String),
    /// Close the batch tag panel.
    Close,
}

// ── Render ───────────────────────────────────────────────────────────────────

/// Render the batch tag panel overlay.
///
/// Panel width is capped at 48 columns and positioned at the top-left of `area`.
pub fn render_batch_tag(frame: &mut Frame, area: Rect, state: &BatchTagPanelFullState) {
    let panel_width = area.width.min(48);
    let panel_height = compute_panel_height(state);

    let panel_area = Rect {
        x: area.x,
        y: area.y,
        width: panel_width,
        height: panel_height.min(area.height),
    };

    // ── Title ────────────────────────────────────────────────────────────
    let n = state.selected_record_ids.len();
    let title = format!(" 批量标签 — 已选 {n} 项 ");

    let outer_block = Block::default()
        .title(title)
        .title_style(Style::default().fg(theme::TEXT))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::BG));

    let inner = outer_block.inner(panel_area);
    frame.render_widget(Clear, panel_area);
    frame.render_widget(outer_block, panel_area);

    // ── Layout inside panel ──────────────────────────────────────────────
    // Sections: input (3 rows) + current tags (variable) + available tags (variable) + done (3 rows)
    let current_tags_rows = tag_display_rows(&state.current_tags, inner.width);
    let available_tags_rows = tag_display_rows(&state.available_tags, inner.width);

    let constraints = vec![
        Constraint::Length(3),                       // Input section
        Constraint::Length(current_tags_rows + 2),   // Current tags section
        Constraint::Length(available_tags_rows + 2),  // Available tags section
        Constraint::Length(3),                       // Done button section
    ];

    let chunks = Layout::vertical(constraints).split(inner);

    // ── Input section ────────────────────────────────────────────────────
    render_input_section(frame, chunks[0], state);

    // ── Current tags section ─────────────────────────────────────────────
    render_tags_section(
        frame,
        chunks[1],
        "当前标签:",
        &state.current_tags,
        state.focus == TagPanelFocus::CurrentTags,
        state.tag_cursor,
    );

    // ── Available tags section ───────────────────────────────────────────
    let available_cursor = if state.focus == TagPanelFocus::AvailableTags {
        state.tag_cursor
    } else {
        usize::MAX // no highlight
    };
    render_tags_section(
        frame,
        chunks[2],
        "可用标签:",
        &state.available_tags,
        state.focus == TagPanelFocus::AvailableTags,
        available_cursor,
    );

    // ── Done button ──────────────────────────────────────────────────────
    render_done_button(frame, chunks[3], state);
}

/// Compute total panel height based on content.
fn compute_panel_height(state: &BatchTagPanelFullState) -> u16 {
    // Outer borders: 2 rows (top + bottom)
    // Input: 3 rows (border top + content + border bottom)
    // Current tags header + content + bottom border
    // Available tags header + content + bottom border
    // Done: 3 rows
    let current_rows = tag_display_rows(&state.current_tags, 46); // 48 - 2 borders
    let available_rows = tag_display_rows(&state.available_tags, 46);

    3 + (current_rows + 2) + (available_rows + 2) + 3 + 2 // +2 for outer borders
}

/// Calculate how many rows are needed to display tags within a given width.
fn tag_display_rows(tags: &[String], max_width: u16) -> u16 {
    if tags.is_empty() {
        return 1; // "  (无)" placeholder
    }
    // Each tag takes up len + 2 (spaces/padding) characters
    let mut rows: u16 = 1;
    let mut col: u16 = 0;
    let usable = max_width.saturating_sub(2); // inner padding

    for tag in tags {
        let tag_width = tag.len() as u16 + 2; // tag text + spacing
        if col + tag_width > usable && col > 0 {
            rows = rows.saturating_add(1);
            col = tag_width;
        } else {
            col = col.saturating_add(tag_width);
        }
    }
    rows.max(1)
}

/// Render the input section with a bordered text field.
fn render_input_section(frame: &mut Frame, area: Rect, state: &BatchTagPanelFullState) {
    let focused = state.focus == TagPanelFocus::Input;
    let border_color = if focused {
        theme::PRIMARY
    } else {
        theme::BORDER
    };

    let input_block = Block::default()
        .title(" 添加标签: ")
        .title_style(Style::default().fg(theme::TEXT_SECONDARY))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(theme::BG));

    let inner = input_block.inner(area);
    frame.render_widget(input_block, area);

    // Build input display with cursor indicator
    let input_text = &state.input_text;
    let display_text = if focused {
        format!("{input_text}█ Enter 添加")
    } else {
        input_text.clone()
    };

    let input_style = Style::default().fg(theme::TEXT).bg(theme::BG);
    let input = Paragraph::new(display_text)
        .style(input_style)
        .wrap(Wrap { trim: false });
    frame.render_widget(input, inner);
}

/// Render a tags section (current or available) with header and tag chips.
fn render_tags_section(
    frame: &mut Frame,
    area: Rect,
    header: &str,
    tags: &[String],
    focused: bool,
    cursor: usize,
) {
    let border_color = if focused {
        theme::PRIMARY
    } else {
        theme::BORDER
    };

    let section_block = Block::default()
        .title(format!(" {header} "))
        .title_style(Style::default().fg(theme::TEXT_SECONDARY))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(theme::BG));

    let inner = section_block.inner(area);
    frame.render_widget(section_block, area);

    if tags.is_empty() {
        let empty = Paragraph::new("  (无)")
            .style(Style::default().fg(theme::TEXT_MUTED))
            .wrap(Wrap { trim: false });
        frame.render_widget(empty, inner);
        return;
    }

    // Build tag spans with highlighting
    let mut spans: Vec<Span> = Vec::new();
    for (i, tag) in tags.iter().enumerate() {
        let tag_style = if focused && i == cursor {
            Style::default()
                .fg(theme::BRAND)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::BRAND)
        };
        spans.push(Span::styled(format!(" {tag} "), tag_style));
    }

    let tags_line = Line::from(spans);
    let paragraph = Paragraph::new(vec![tags_line]).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

/// Render the done button at the bottom.
fn render_done_button(frame: &mut Frame, area: Rect, state: &BatchTagPanelFullState) {
    let focused = state.focus == TagPanelFocus::DoneButton;

    let button_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::BG));

    let inner = button_block.inner(area);
    frame.render_widget(button_block, area);

    let button_text = " 完成 ";
    let button_style = if focused {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    };

    let label = Paragraph::new(button_text)
        .style(button_style)
        .wrap(Wrap { trim: false });
    frame.render_widget(label, inner);
}

// ── Keyboard handling ────────────────────────────────────────────────────────

/// Handle a key event and return the action the caller should dispatch.
///
/// The function mutates `state` in place for internal navigation (focus changes,
/// cursor movement, text editing) and returns actions requiring external dispatch.
pub fn handle_key(key: KeyCode, state: &mut BatchTagPanelFullState) -> BatchTagAction {
    match state.focus {
        TagPanelFocus::Input => handle_input_focus(key, state),
        TagPanelFocus::CurrentTags => handle_current_tags_focus(key, state),
        TagPanelFocus::AvailableTags => handle_available_tags_focus(key, state),
        TagPanelFocus::DoneButton => handle_done_button_focus(key, state),
    }
}

/// Advance focus to the next zone in cycle order.
fn next_focus(state: &mut BatchTagPanelFullState) {
    state.focus = match state.focus {
        TagPanelFocus::Input => TagPanelFocus::CurrentTags,
        TagPanelFocus::CurrentTags => TagPanelFocus::AvailableTags,
        TagPanelFocus::AvailableTags => TagPanelFocus::DoneButton,
        TagPanelFocus::DoneButton => TagPanelFocus::Input,
    };
    // Reset cursor when entering a tag list section
    state.tag_cursor = 0;
}

/// Move focus to the previous zone in cycle order.
fn prev_focus(state: &mut BatchTagPanelFullState) {
    state.focus = match state.focus {
        TagPanelFocus::Input => TagPanelFocus::DoneButton,
        TagPanelFocus::CurrentTags => TagPanelFocus::Input,
        TagPanelFocus::AvailableTags => TagPanelFocus::CurrentTags,
        TagPanelFocus::DoneButton => TagPanelFocus::AvailableTags,
    };
    // Reset cursor when entering a tag list section
    state.tag_cursor = 0;
}

/// Handle keys when Input zone is focused.
fn handle_input_focus(key: KeyCode, state: &mut BatchTagPanelFullState) -> BatchTagAction {
    match key {
        KeyCode::Char(c) => {
            state.input_text.push(c);
            BatchTagAction::None
        }
        KeyCode::Backspace => {
            state.input_text.pop();
            BatchTagAction::None
        }
        KeyCode::Enter => {
            if !state.input_text.is_empty() {
                let tag = state.input_text.clone();
                state.input_text.clear();
                BatchTagAction::AddTag(tag)
            } else {
                BatchTagAction::None
            }
        }
        KeyCode::Tab => {
            next_focus(state);
            BatchTagAction::None
        }
        KeyCode::Esc => BatchTagAction::Close,
        _ => BatchTagAction::None,
    }
}

/// Handle keys when CurrentTags zone is focused.
fn handle_current_tags_focus(key: KeyCode, state: &mut BatchTagPanelFullState) -> BatchTagAction {
    let count = state.current_tags.len();
    match key {
        KeyCode::Left | KeyCode::Char('h') => {
            if state.tag_cursor > 0 {
                state.tag_cursor -= 1;
            }
            BatchTagAction::None
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if state.tag_cursor + 1 < count {
                state.tag_cursor += 1;
            }
            BatchTagAction::None
        }
        KeyCode::Backspace | KeyCode::Char('d') => {
            if count > 0 && state.tag_cursor < count {
                let tag = state.current_tags[state.tag_cursor].clone();
                state.current_tags.remove(state.tag_cursor);
                // Adjust cursor if now out of bounds
                if state.tag_cursor > 0 && state.tag_cursor >= state.current_tags.len() {
                    state.tag_cursor = state.current_tags.len().saturating_sub(1);
                }
                BatchTagAction::RemoveTag(tag)
            } else {
                BatchTagAction::None
            }
        }
        KeyCode::Tab => {
            next_focus(state);
            BatchTagAction::None
        }
        KeyCode::BackTab => {
            prev_focus(state);
            BatchTagAction::None
        }
        KeyCode::Esc => BatchTagAction::Close,
        _ => BatchTagAction::None,
    }
}

/// Handle keys when AvailableTags zone is focused.
fn handle_available_tags_focus(
    key: KeyCode,
    state: &mut BatchTagPanelFullState,
) -> BatchTagAction {
    let count = state.available_tags.len();
    match key {
        KeyCode::Left | KeyCode::Char('h') => {
            if state.tag_cursor > 0 {
                state.tag_cursor -= 1;
            }
            BatchTagAction::None
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if state.tag_cursor + 1 < count {
                state.tag_cursor += 1;
            }
            BatchTagAction::None
        }
        KeyCode::Char(' ') | KeyCode::Enter => {
            if count > 0 && state.tag_cursor < count {
                let tag = state.available_tags[state.tag_cursor].clone();
                BatchTagAction::AddTag(tag)
            } else {
                BatchTagAction::None
            }
        }
        KeyCode::Tab => {
            next_focus(state);
            BatchTagAction::None
        }
        KeyCode::BackTab => {
            prev_focus(state);
            BatchTagAction::None
        }
        KeyCode::Esc => BatchTagAction::Close,
        _ => BatchTagAction::None,
    }
}

/// Handle keys when DoneButton zone is focused.
fn handle_done_button_focus(key: KeyCode, state: &mut BatchTagPanelFullState) -> BatchTagAction {
    match key {
        KeyCode::Enter => BatchTagAction::Close,
        KeyCode::Tab => {
            next_focus(state);
            BatchTagAction::None
        }
        KeyCode::BackTab => {
            prev_focus(state);
            BatchTagAction::None
        }
        KeyCode::Esc => BatchTagAction::Close,
        _ => BatchTagAction::None,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::overlay_state::TagPanelFocus;
    use uuid::Uuid;

    /// Helper: create a default state for testing.
    fn test_state() -> BatchTagPanelFullState {
        BatchTagPanelFullState {
            selected_record_ids: vec![Uuid::new_v4(), Uuid::new_v4()],
            selected_record_names: vec!["Record A".into(), "Record B".into()],
            input_text: String::new(),
            current_tags: vec!["work".into(), "personal".into()],
            available_tags: vec!["finance".into(), "social".into(), "travel".into()],
            focus: TagPanelFocus::Input,
            tag_cursor: 0,
            current_tag: String::new(),
        }
    }

    #[test]
    fn input_char_appends() {
        let mut state = test_state();
        state.focus = TagPanelFocus::Input;

        let result = handle_key(KeyCode::Char('a'), &mut state);
        assert_eq!(result, BatchTagAction::None);
        assert_eq!(state.input_text, "a");

        let result = handle_key(KeyCode::Char('b'), &mut state);
        assert_eq!(result, BatchTagAction::None);
        assert_eq!(state.input_text, "ab");

        let result = handle_key(KeyCode::Char('c'), &mut state);
        assert_eq!(result, BatchTagAction::None);
        assert_eq!(state.input_text, "abc");
    }

    #[test]
    fn input_backspace_removes() {
        let mut state = test_state();
        state.focus = TagPanelFocus::Input;
        state.input_text = "hello".into();

        let result = handle_key(KeyCode::Backspace, &mut state);
        assert_eq!(result, BatchTagAction::None);
        assert_eq!(state.input_text, "hell");

        // Backspace on empty string is a no-op
        state.input_text.clear();
        let result = handle_key(KeyCode::Backspace, &mut state);
        assert_eq!(result, BatchTagAction::None);
        assert_eq!(state.input_text, "");
    }

    #[test]
    fn input_enter_adds_tag() {
        let mut state = test_state();
        state.focus = TagPanelFocus::Input;
        state.input_text = "newtag".into();

        let result = handle_key(KeyCode::Enter, &mut state);
        assert_eq!(result, BatchTagAction::AddTag("newtag".into()));
        assert!(state.input_text.is_empty(), "input should be cleared after Enter");

        // Enter on empty input does nothing
        let result = handle_key(KeyCode::Enter, &mut state);
        assert_eq!(result, BatchTagAction::None);
    }

    #[test]
    fn tab_cycles_focus() {
        let mut state = test_state();

        // Start at Input
        assert_eq!(state.focus, TagPanelFocus::Input);

        handle_key(KeyCode::Tab, &mut state);
        assert_eq!(state.focus, TagPanelFocus::CurrentTags);

        handle_key(KeyCode::Tab, &mut state);
        assert_eq!(state.focus, TagPanelFocus::AvailableTags);

        handle_key(KeyCode::Tab, &mut state);
        assert_eq!(state.focus, TagPanelFocus::DoneButton);

        // Tab wraps back to Input
        handle_key(KeyCode::Tab, &mut state);
        assert_eq!(state.focus, TagPanelFocus::Input);
    }

    #[test]
    fn esc_closes_from_any_focus() {
        for focus in [
            TagPanelFocus::Input,
            TagPanelFocus::CurrentTags,
            TagPanelFocus::AvailableTags,
            TagPanelFocus::DoneButton,
        ] {
            let mut state = test_state();
            state.focus = focus;
            let result = handle_key(KeyCode::Esc, &mut state);
            assert_eq!(
                result,
                BatchTagAction::Close,
                "Esc should close from {:?}",
                focus
            );
        }
    }
}
