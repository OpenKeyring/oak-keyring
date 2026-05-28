//! Standalone password generator overlay (Sidebar trigger).

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::t;
use crate::tui::components::generator_panel;
use crate::tui::state::generator_state::{GenerationStyle, GeneratorFocus, GeneratorState};
use crate::tui::theme;

/// Render the standalone generator overlay.
pub fn render_generator(frame: &mut Frame, area: Rect, state: &GeneratorState, unicode: bool) {
    let dialog_w: u16 = 60.min(area.width);
    let dialog_area = centered_rect(dialog_w, area);

    let mut lines = vec![title_line(dialog_w), separator_line(dialog_w), Line::raw("")];

    // Generator panel content
    let panel_lines = generator_panel::render_generator_panel(state, false, dialog_w, unicode);
    lines.extend(panel_lines);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::NL_FOCUS))
        .style(Style::default().bg(theme::NL_SURFACE));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .style(theme::Styles::newlook_surface())
        .wrap(Wrap { trim: false });

    frame.render_widget(Clear, dialog_area);
    frame.render_widget(paragraph, dialog_area);
}

/// Actions from standalone generator key handling.
pub enum GeneratorAction {
    None,
    Regenerate,
    CopyToClipboard,
    Close,
}

/// Handle keyboard input for standalone generator.
pub fn handle_key(key: crossterm::event::KeyCode, state: &mut GeneratorState) -> GeneratorAction {
    use crossterm::event::KeyCode;
    match key {
        KeyCode::Down => {
            state.focus_section_down();
            GeneratorAction::None
        }
        KeyCode::Up => {
            state.focus_section_up();
            GeneratorAction::None
        }
        KeyCode::Tab => {
            state.focus_next();
            GeneratorAction::None
        }
        KeyCode::BackTab => {
            state.focus_prev();
            GeneratorAction::None
        }
        KeyCode::Esc => GeneratorAction::Close,
        KeyCode::Char('r') => {
            state.regenerate();
            GeneratorAction::Regenerate
        }
        KeyCode::Char('c') => {
            if state.focus == GeneratorFocus::SeparatorInput {
                state.memorable_config.separator = "c".to_string();
                state.regenerate();
                GeneratorAction::None
            } else {
                GeneratorAction::CopyToClipboard
            }
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            if state.focus == GeneratorFocus::SeparatorInput {
                state.memorable_config.separator = "+".to_string();
                state.regenerate();
            } else {
                state.increment_length();
            }
            GeneratorAction::None
        }
        KeyCode::Char('-') => {
            if state.focus == GeneratorFocus::SeparatorInput {
                state.memorable_config.separator = "-".to_string();
                state.regenerate();
            } else {
                state.decrement_length();
            }
            GeneratorAction::None
        }
        KeyCode::Enter => match state.focus {
            GeneratorFocus::ActionButton => GeneratorAction::CopyToClipboard,
            GeneratorFocus::RegenerateButton => {
                state.regenerate();
                GeneratorAction::Regenerate
            }
            GeneratorFocus::Toggle(idx) => {
                toggle_focused_option(idx, state);
                GeneratorAction::None
            }
            _ => GeneratorAction::None,
        },
        KeyCode::Char(' ') => match state.focus {
            GeneratorFocus::Toggle(idx) => {
                toggle_focused_option(idx, state);
                GeneratorAction::None
            }
            GeneratorFocus::ActionButton => GeneratorAction::CopyToClipboard,
            GeneratorFocus::RegenerateButton => {
                state.regenerate();
                GeneratorAction::Regenerate
            }
            _ => GeneratorAction::None,
        },
        _ => match state.focus {
            GeneratorFocus::StyleSelector => match key {
                KeyCode::Left => {
                    match state.style {
                        GenerationStyle::Random => {}
                        GenerationStyle::Memorable => state.set_style(GenerationStyle::Random),
                        GenerationStyle::Pin => state.set_style(GenerationStyle::Memorable),
                    }
                    GeneratorAction::None
                }
                KeyCode::Right => {
                    match state.style {
                        GenerationStyle::Random => state.set_style(GenerationStyle::Memorable),
                        GenerationStyle::Memorable => state.set_style(GenerationStyle::Pin),
                        GenerationStyle::Pin => {}
                    }
                    GeneratorAction::None
                }
                _ => GeneratorAction::None,
            },
            GeneratorFocus::LengthSlider => match key {
                KeyCode::Left => {
                    state.decrement_length();
                    GeneratorAction::None
                }
                KeyCode::Right => {
                    state.increment_length();
                    GeneratorAction::None
                }
                _ => GeneratorAction::None,
            },
            GeneratorFocus::Toggle(idx) => {
                match key {
                    KeyCode::Left => state.focus_prev_toggle(),
                    KeyCode::Right => state.focus_next_toggle(),
                    _ => {
                        let _ = idx;
                    }
                }
                GeneratorAction::None
            }
            GeneratorFocus::SeparatorInput => match key {
                KeyCode::Left => {
                    state.focus_prev_toggle();
                    GeneratorAction::None
                }
                KeyCode::Right => {
                    state.focus_next_toggle();
                    GeneratorAction::None
                }
                KeyCode::Char(c) => {
                    state.memorable_config.separator = c.to_string();
                    state.regenerate();
                    GeneratorAction::None
                }
                KeyCode::Backspace => {
                    state.memorable_config.separator = "-".to_string();
                    state.regenerate();
                    GeneratorAction::None
                }
                _ => GeneratorAction::None,
            },
            GeneratorFocus::RegenerateButton => match key {
                KeyCode::Right => {
                    state.focus = GeneratorFocus::ActionButton;
                    GeneratorAction::None
                }
                _ => GeneratorAction::None,
            },
            GeneratorFocus::ActionButton => match key {
                KeyCode::Left => {
                    state.focus = GeneratorFocus::RegenerateButton;
                    GeneratorAction::None
                }
                _ => GeneratorAction::None,
            },
        },
    }
}

fn toggle_focused_option(idx: usize, state: &mut GeneratorState) {
    if state.style == GenerationStyle::Random {
        state.toggle_char_type(idx);
    } else if state.style == GenerationStyle::Memorable && idx == 0 {
        state.memorable_config.capitalize = !state.memorable_config.capitalize;
        state.regenerate();
    }
}

fn title_line(dialog_width: u16) -> Line<'static> {
    let content_width = dialog_width.saturating_sub(2) as usize;
    let title = t!("tui.generator_overlay.title").to_string();
    let hint = t!("tui.generator_overlay.close_hint").to_string();
    let used = UnicodeWidthStr::width(title.as_str()) + UnicodeWidthStr::width(hint.as_str()) + 2;
    let spacer = content_width.saturating_sub(used);
    Line::from(vec![
        Span::styled(
            title,
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(spacer)),
        Span::styled(hint, Style::default().fg(theme::TEXT_SECONDARY)),
        Span::raw("  "),
    ])
}

fn separator_line(width: u16) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width.saturating_sub(2) as usize),
        Style::default().fg(theme::BORDER),
    ))
}

fn centered_rect(width: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(20)) / 2;
    Rect::new(x, y, width, 22.min(area.height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn handle_tab_cycles_focus() {
        let mut state = GeneratorState::new();
        let initial = state.focus;
        handle_key(KeyCode::Tab, &mut state);
        assert_ne!(state.focus, initial);
    }

    #[test]
    fn handle_down_and_up_move_focus() {
        let mut state = GeneratorState::new();
        let initial = state.focus;
        handle_key(KeyCode::Down, &mut state);
        assert_ne!(state.focus, initial);
        handle_key(KeyCode::Up, &mut state);
        assert_eq!(state.focus, initial);
    }

    #[test]
    fn handle_esc_closes() {
        let mut state = GeneratorState::new();
        assert!(matches!(
            handle_key(KeyCode::Esc, &mut state),
            GeneratorAction::Close
        ));
    }

    #[test]
    fn handle_r_regenerates() {
        let mut state = GeneratorState::new();
        assert!(matches!(
            handle_key(KeyCode::Char('r'), &mut state),
            GeneratorAction::Regenerate
        ));
    }

    #[test]
    fn handle_enter_on_action_copies() {
        let mut state = GeneratorState::new();
        state.focus = GeneratorFocus::ActionButton;
        assert!(matches!(
            handle_key(KeyCode::Enter, &mut state),
            GeneratorAction::CopyToClipboard
        ));
    }

    #[test]
    fn handle_enter_on_toggle_updates_random_option() {
        let mut state = GeneratorState::new();
        state.focus = GeneratorFocus::Toggle(2);
        assert!(state.random_config.digits);
        assert!(matches!(
            handle_key(KeyCode::Enter, &mut state),
            GeneratorAction::None
        ));
        assert!(!state.random_config.digits);
    }

    #[test]
    fn handle_right_switches_style() {
        let mut state = GeneratorState::new();
        state.focus = GeneratorFocus::StyleSelector;
        handle_key(KeyCode::Right, &mut state);
        assert_eq!(state.style, GenerationStyle::Memorable);
    }

    #[test]
    fn handle_left_on_length_decrements() {
        let mut state = GeneratorState::new();
        state.focus = GeneratorFocus::LengthSlider;
        state.random_config.length = 20;
        handle_key(KeyCode::Left, &mut state);
        assert_eq!(state.random_config.length, 19);
    }
}
