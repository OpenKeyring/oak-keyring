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

use crate::t;
use crate::tui::state::overlay_state::PasswordHistoryState;
use crate::tui::theme;

// ── Colour constants ──────────────────────────────────────────

const OVERLAY_BG: Color = Color::Rgb(20, 24, 39); // #141827

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

    let body_lines = build_body_lines(state);
    let footer_line = build_footer_line(state);
    let close_button = build_title_line();

    // +2 for borders (top/bottom), +1 blank separator before footer
    let total_lines = body_lines.len() + 2;
    let height = (total_lines as u16 + 2).min(area.height);
    let overlay_rect = centered_rect(area, width, height);

    let title = format!(
        " {} ",
        t!("tui.history.title", name = state.record_name.as_str())
    );
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme::NL_TEXT)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::NL_FOCUS))
        .style(Style::default().bg(OVERLAY_BG))
        .title_bottom(Span::styled(
            close_button,
            Style::default().fg(theme::NL_TEXT_MUTED),
        ));

    let mut all_lines = body_lines;
    all_lines.push(footer_line);

    let paragraph = Paragraph::new(all_lines)
        .block(block)
        .style(theme::Styles::newlook_surface())
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
    super::centered_rect(area, width, height)
}

// ── Rendering helpers ─────────────────────────────────────────

/// Build the body lines (entry list or empty state).
fn build_body_lines(state: &PasswordHistoryState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if state.entries.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {}", t!("tui.history.no_history")),
            Style::default().fg(theme::NL_LINE),
        )));
        return lines;
    }

    for (i, entry) in state.entries.iter().enumerate() {
        let is_selected = i == state.selected_index;

        let date_str = entry.changed_at.format("%Y-%m-%d %H:%M").to_string();

        let date_style = if is_selected {
            Style::default()
                .fg(theme::NL_TEXT_MUTED)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::NL_TEXT_MUTED)
        };

        let desc_style = if is_selected {
            Style::default()
                .fg(theme::NL_TEXT)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::NL_TEXT)
        };

        let copy_style = if is_selected {
            Style::default()
                .fg(theme::NL_CYAN)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::NL_CYAN)
        };

        lines.push(Line::from(vec![
            Span::styled(" ", date_style),
            Span::styled(date_str, date_style),
            Span::styled("  ", desc_style),
            Span::styled(entry.description.clone(), desc_style),
            Span::styled("  ", copy_style),
            Span::styled(t!("tui.history.copy_button"), copy_style),
        ]));
    }

    lines
}

/// Build the footer line showing the record count.
fn build_footer_line(state: &PasswordHistoryState) -> Line<'static> {
    let n = state.entries.len();
    Line::from(Span::styled(
        format!(" {}", t!("tui.history.record_count", count = n)),
        Style::default().fg(theme::NL_LINE),
    ))
}

/// Build the close-button text shown in the title area.
fn build_title_line() -> String {
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
                id: i as i64 + 1,
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
