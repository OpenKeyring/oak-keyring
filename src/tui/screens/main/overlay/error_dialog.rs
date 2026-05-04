//! Error dialog overlay — displays an error message with Retry/Quit actions.
//!
//! Renders a centred modal dialog with an error icon, title, message,
//! optional detail text, and action buttons.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::t;
use crate::tui::state::overlay_state::{ErrorActions, ErrorDialogFullState};
use crate::tui::theme;

// ── Colour constants ──────────────────────────────────────────

const OVERLAY_BG: Color = Color::Rgb(26, 27, 38); // #1a1b26

// ── Layout constants ──────────────────────────────────────────

const DIALOG_WIDTH: u16 = 56;

// ── Public types ──────────────────────────────────────────────

/// Result of handling a key event in the error dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorDialogAction {
    /// No action taken (e.g. key not recognised).
    None,
    /// User chose to retry the operation.
    Retry,
    /// User chose to quit / dismiss.
    Quit,
}

// ── Public API ────────────────────────────────────────────────

/// Render the error dialog overlay, centred within `area`.
pub fn render_error_dialog(frame: &mut Frame, area: Rect, state: &ErrorDialogFullState) {
    let width = DIALOG_WIDTH.min(area.width);

    let lines = build_body_lines(state);
    let button_line = build_button_line(state);

    let total_lines = lines.len() + 2; // +1 blank line +1 button line
    let height = (total_lines as u16 + 2).min(area.height); // +2 for border top/bottom
    let overlay_rect = centered_rect(area, width, height);

    let title = format!(" {} ", t!("tui.error.fatal_title"));
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme::ERROR)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(OVERLAY_BG));

    // Build final content: body lines + blank separator + button line
    let mut all_lines = lines;
    all_lines.push(Line::from(""));
    all_lines.push(button_line);

    let paragraph = Paragraph::new(all_lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Center);

    frame.render_widget(Clear, overlay_rect);
    frame.render_widget(paragraph, overlay_rect);
}

/// Handle a key press in the error dialog, returning the action to take.
pub fn handle_key(
    key: crossterm::event::KeyCode,
    state: &mut ErrorDialogFullState,
) -> ErrorDialogAction {
    use crossterm::event::KeyCode;

    match key {
        KeyCode::Esc => ErrorDialogAction::Quit,

        KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
            // Only toggle for RetryQuit variant.
            if matches!(state.actions, ErrorActions::RetryQuit) {
                state.focused_button = if state.focused_button == 0 { 1 } else { 0 };
            }
            ErrorDialogAction::None
        }

        KeyCode::Enter => match state.actions {
            ErrorActions::RetryQuit => {
                if state.focused_button == 0 {
                    ErrorDialogAction::Retry
                } else {
                    ErrorDialogAction::Quit
                }
            }
            ErrorActions::QuitOnly => ErrorDialogAction::Quit,
        },

        _ => ErrorDialogAction::None,
    }
}

// ── Layout helpers ────────────────────────────────────────────

/// Return a `Rect` of size `width x height` centred inside `area`.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    super::centered_rect(area, width, height)
}

// ── Rendering helpers ─────────────────────────────────────────

/// Build the body lines (message + optional detail).
fn build_body_lines(state: &ErrorDialogFullState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Error message
    lines.push(Line::from(Span::styled(
        state.message.clone(),
        Style::default().fg(theme::TEXT_SECONDARY),
    )));

    // Optional detail
    if let Some(ref detail) = state.detail {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            detail.clone(),
            Style::default().fg(theme::TEXT_MUTED),
        )));
    }

    lines
}

/// Build the button line with proper focus styling.
fn build_button_line(state: &ErrorDialogFullState) -> Line<'static> {
    match state.actions {
        ErrorActions::RetryQuit => {
            let retry_label = format!(" {} ", t!("tui.error.retry"));
            let quit_label = format!(" {} ", t!("tui.error.exit"));

            let retry_style = if state.focused_button == 0 {
                Style::default()
                    .fg(theme::PRIMARY)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(theme::PRIMARY)
            };

            let quit_style = if state.focused_button == 1 {
                Style::default()
                    .fg(theme::TEXT_SECONDARY)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(theme::TEXT_SECONDARY)
            };

            Line::from(vec![
                Span::styled(retry_label, retry_style),
                Span::raw("  "),
                Span::styled(quit_label, quit_style),
            ])
        }
        ErrorActions::QuitOnly => {
            let label = format!(" {} ", t!("tui.error.exit"));
            let style = Style::default()
                .fg(theme::TEXT_SECONDARY)
                .add_modifier(Modifier::REVERSED);
            Line::from(vec![Span::styled(label, style)])
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::overlay_state::ErrorActions;

    fn retry_quit_state(focused: usize) -> ErrorDialogFullState {
        ErrorDialogFullState {
            title: "Error".to_string(),
            message: "Something went wrong".to_string(),
            detail: None,
            actions: ErrorActions::RetryQuit,
            focused_button: focused,
        }
    }

    fn quit_only_state() -> ErrorDialogFullState {
        ErrorDialogFullState {
            title: "Error".to_string(),
            message: "Fatal error".to_string(),
            detail: None,
            actions: ErrorActions::QuitOnly,
            focused_button: 0,
        }
    }

    #[test]
    fn tab_toggles_between_retry_quit() {
        let mut state = retry_quit_state(0);
        handle_key(crossterm::event::KeyCode::Tab, &mut state);
        assert_eq!(state.focused_button, 1);

        handle_key(crossterm::event::KeyCode::Tab, &mut state);
        assert_eq!(state.focused_button, 0);

        // Also test Left/Right arrows.
        handle_key(crossterm::event::KeyCode::Right, &mut state);
        assert_eq!(state.focused_button, 1);

        handle_key(crossterm::event::KeyCode::Left, &mut state);
        assert_eq!(state.focused_button, 0);
    }

    #[test]
    fn enter_on_first_button_retries() {
        let mut state = retry_quit_state(0);
        let action = handle_key(crossterm::event::KeyCode::Enter, &mut state);
        assert_eq!(action, ErrorDialogAction::Retry);
    }

    #[test]
    fn enter_on_second_button_quits() {
        let mut state = retry_quit_state(1);
        let action = handle_key(crossterm::event::KeyCode::Enter, &mut state);
        assert_eq!(action, ErrorDialogAction::Quit);
    }

    #[test]
    fn esc_always_quits() {
        let mut state = retry_quit_state(0);
        assert_eq!(
            handle_key(crossterm::event::KeyCode::Esc, &mut state),
            ErrorDialogAction::Quit
        );

        let mut state = quit_only_state();
        assert_eq!(
            handle_key(crossterm::event::KeyCode::Esc, &mut state),
            ErrorDialogAction::Quit
        );
    }

    #[test]
    fn quit_only_enter_quits() {
        let mut state = quit_only_state();
        let action = handle_key(crossterm::event::KeyCode::Enter, &mut state);
        assert_eq!(action, ErrorDialogAction::Quit);
    }
}
