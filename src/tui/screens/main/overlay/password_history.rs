//! Password history overlay — displays a record's password change history.
//!
//! Renders a centred modal dialog with a scrollable list of history entries,
//! each showing a timestamp, description, and a copy button.

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::tui::state::overlay_state::PasswordHistoryState;
use crate::tui::theme;

// ── Colour constants ──────────────────────────────────────────

const OVERLAY_BG: Color = Color::Rgb(26, 27, 38); // #1a1b26

// ── Layout constants ──────────────────────────────────────────

const DIALOG_WIDTH: u16 = 56;

// ── Public types ──────────────────────────────────────────────

/// Result of handling a key event in the password history overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryAction {
    /// No action taken (unrecognised key).
    None,
    /// Move selection up.
    MoveUp,
    /// Move selection down.
    MoveDown,
    /// Copy the selected entry to clipboard.
    CopySelected,
    /// Close the overlay.
    Close,
}

// ── Public API ────────────────────────────────────────────────

/// Render the password history overlay, centred within `area`.
pub fn render_password_history(frame: &mut Frame, area: Rect, state: &PasswordHistoryState) {
    let width = DIALOG_WIDTH.min(area.width);
    let content_width = width.saturating_sub(2); // subtract borders

    let body_lines = build_body_lines(state, content_width);
    let footer_line = build_footer_line(state);
    let close_button = build_title_line(state);

    // +2 for borders (top/bottom), +1 blank separator before footer
    let total_lines = body_lines.len() + 2;
    let height = (total_lines as u16 + 2).min(area.height);
    let overlay_rect = centered_rect(area, width, height);

    let title = format!(" 密码历史 \u{2014} {} ", state.record_name);
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(OVERLAY_BG))
        .title_bottom(Span::styled(
            close_button,
            Style::default().fg(theme::TEXT_SECONDARY),
        ));

    let mut all_lines = body_lines;
    all_lines.push(footer_line);

    let paragraph = Paragraph::new(all_lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(Clear, overlay_rect);
    frame.render_widget(paragraph, overlay_rect);
}

/// Handle a key press in the password history overlay, returning the action to take.
pub fn handle_key(key: KeyCode, state: &mut PasswordHistoryState) -> HistoryAction {
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            if state.selected_index > 0 {
                state.selected_index -= 1;
            }
            HistoryAction::MoveUp
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let last = state.entries.len().saturating_sub(1);
            if state.selected_index < last {
                state.selected_index += 1;
            }
            HistoryAction::MoveDown
        }
        KeyCode::Enter | KeyCode::Char('c') => {
            if !state.entries.is_empty() {
                HistoryAction::CopySelected
            } else {
                HistoryAction::None
            }
        }
        KeyCode::Esc | KeyCode::Char('H') => HistoryAction::Close,
        _ => HistoryAction::None,
    }
}

// ── Layout helpers ────────────────────────────────────────────

/// Return a `Rect` of size `width x height` centred inside `area`.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let x = area
        .x
        .checked_add((area.width.saturating_sub(width)) / 2)
        .unwrap_or(area.x);
    let y = area
        .y
        .checked_add((area.height.saturating_sub(height)) / 2)
        .unwrap_or(area.y);
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

// ── Rendering helpers ─────────────────────────────────────────

/// Build the body lines (entry list or empty state).
fn build_body_lines(state: &PasswordHistoryState, _content_width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if state.entries.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  暂无历史记录",
            Style::default().fg(theme::TEXT_MUTED),
        )));
        return lines;
    }

    for (i, entry) in state.entries.iter().enumerate() {
        let is_selected = i == state.selected_index;

        let date_str = entry.changed_at.format("%Y-%m-%d %H:%M").to_string();

        let date_style = if is_selected {
            Style::default()
                .fg(theme::TEXT_SECONDARY)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::TEXT_SECONDARY)
        };

        let desc_style = if is_selected {
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::TEXT)
        };

        let copy_style = if is_selected {
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::PRIMARY)
        };

        lines.push(Line::from(vec![
            Span::styled(" ", date_style),
            Span::styled(date_str, date_style),
            Span::styled("  ", desc_style),
            Span::styled(entry.description.clone(), desc_style),
            Span::styled("  ", copy_style),
            Span::styled("[复制]", copy_style),
        ]));
    }

    lines
}

/// Build the footer line showing the record count.
fn build_footer_line(state: &PasswordHistoryState) -> Line<'static> {
    let n = state.entries.len();
    Line::from(Span::styled(
        format!(" 共 {n} 条记录（最多保留 10 条）"),
        Style::default().fg(theme::TEXT_MUTED),
    ))
}

/// Build the close-button text shown in the title area.
fn build_title_line(_state: &PasswordHistoryState) -> String {
    " ✕ ".to_string()
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn make_state(n_entries: usize, selected: usize) -> PasswordHistoryState {
        let entries: Vec<_> = (0..n_entries)
            .map(|i| crate::tui::state::overlay_state::HistoryEntry {
                changed_at: Utc.with_ymd_and_hms(2025, 1, 1, 12, 0, i as u32).unwrap(),
                description: format!("password v{}", i + 1),
            })
            .collect();
        PasswordHistoryState {
            record_id: Uuid::new_v4(),
            record_name: "test-record".to_string(),
            entries,
            selected_index: selected,
        }
    }

    #[test]
    fn handle_key_down_moves_selection() {
        let mut state = make_state(3, 0);
        let action = handle_key(KeyCode::Down, &mut state);
        assert_eq!(action, HistoryAction::MoveDown);
        assert_eq!(state.selected_index, 1);
    }

    #[test]
    fn handle_key_up_does_not_underflow() {
        let mut state = make_state(3, 0);
        let action = handle_key(KeyCode::Up, &mut state);
        assert_eq!(action, HistoryAction::MoveUp);
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn handle_key_down_clamps_at_last() {
        let mut state = make_state(3, 2);
        let action = handle_key(KeyCode::Down, &mut state);
        assert_eq!(action, HistoryAction::MoveDown);
        assert_eq!(state.selected_index, 2);
    }

    #[test]
    fn handle_key_enter_returns_copy() {
        let mut state = make_state(3, 1);
        let action = handle_key(KeyCode::Enter, &mut state);
        assert_eq!(action, HistoryAction::CopySelected);
    }

    #[test]
    fn handle_key_esc_returns_close() {
        let mut state = make_state(3, 1);
        let action = handle_key(KeyCode::Esc, &mut state);
        assert_eq!(action, HistoryAction::Close);
    }
}
