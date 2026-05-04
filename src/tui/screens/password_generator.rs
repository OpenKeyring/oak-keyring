//! Standalone password generator screen (U6).
//!
//! Full-screen password generator with style selector, length slider,
//! character toggles, strength indicator, and copy/regenerate actions.
//! Reuses `GeneratorState` and `generator_panel` from the overlay version.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::commands::{Command, Message};
use crate::tui::components::generator_panel;
use crate::tui::state::generator_state::{GenerationStyle, GeneratorFocus, GeneratorState};
use crate::tui::theme;
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};

pub struct PasswordGeneratorScreen {
    pub state: GeneratorState,
    pub hint_message: Option<String>,
}

impl PasswordGeneratorScreen {
    pub fn new() -> Self {
        Self {
            state: GeneratorState::new(),
            hint_message: None,
        }
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Tab => {
                self.hint_message = None;
                self.state.focus_next();
                ScreenResult::Continue
            }
            KeyCode::BackTab => {
                self.hint_message = None;
                self.state.focus_prev();
                ScreenResult::Continue
            }
            KeyCode::Esc => ScreenResult::PopScreen,
            KeyCode::Char('r') => {
                self.hint_message = None;
                self.state.regenerate();
                ScreenResult::Continue
            }
            KeyCode::Enter => self.handle_enter(ctx),
            _ => self.handle_focus_key(key.code),
        }
    }

    fn handle_enter(&mut self, ctx: &mut ScreenContext) -> ScreenResult {
        match self.state.focus {
            GeneratorFocus::ActionButton => {
                let pw = std::mem::take(&mut self.state.preview);
                if !pw.is_empty() {
                    use crate::types::sensitive::SecureStr;
                    let _ = ctx.command_tx.try_send(Command::CopyRawToClipboard {
                        value: SecureStr::new(pw),
                    });
                    self.state.regenerate();
                    self.hint_message = Some("已复制到剪贴板".to_string());
                }
                ScreenResult::Continue
            }
            GeneratorFocus::RegenerateButton => {
                self.state.regenerate();
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_focus_key(&mut self, key: KeyCode) -> ScreenResult {
        match self.state.focus {
            GeneratorFocus::StyleSelector => match key {
                KeyCode::Left => {
                    self.hint_message = None;
                    match self.state.style {
                        GenerationStyle::Random => {}
                        GenerationStyle::Memorable => self.state.set_style(GenerationStyle::Random),
                        GenerationStyle::Pin => self.state.set_style(GenerationStyle::Memorable),
                    }
                    ScreenResult::Continue
                }
                KeyCode::Right => {
                    self.hint_message = None;
                    match self.state.style {
                        GenerationStyle::Random => self.state.set_style(GenerationStyle::Memorable),
                        GenerationStyle::Memorable => self.state.set_style(GenerationStyle::Pin),
                        GenerationStyle::Pin => {}
                    }
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            GeneratorFocus::LengthSlider => match key {
                KeyCode::Left | KeyCode::Char('-') => {
                    self.hint_message = None;
                    self.state.decrement_length();
                    ScreenResult::Continue
                }
                KeyCode::Right | KeyCode::Char('+') => {
                    self.hint_message = None;
                    self.state.increment_length();
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            GeneratorFocus::Toggle(idx) => {
                self.hint_message = None;
                if self.state.style == GenerationStyle::Random {
                    self.state.toggle_char_type(idx);
                } else if self.state.style == GenerationStyle::Memorable && idx == 0 {
                    self.state.memorable_config.capitalize =
                        !self.state.memorable_config.capitalize;
                    self.state.regenerate();
                }
                ScreenResult::Continue
            }
            GeneratorFocus::SeparatorInput => match key {
                KeyCode::Char(c) => {
                    self.hint_message = None;
                    self.state.memorable_config.separator = c.to_string();
                    self.state.regenerate();
                    ScreenResult::Continue
                }
                KeyCode::Backspace => {
                    self.hint_message = None;
                    self.state.memorable_config.separator = "-".to_string();
                    self.state.regenerate();
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            _ => ScreenResult::Continue,
        }
    }

    fn render_title_bar(&self, frame: &mut Frame, area: Rect) {
        let title = " 密码生成器";
        let hint = self.hint_message.as_deref().unwrap_or("");
        let line = Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .bg(theme::BG_BAR)
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "{:>width$}",
                    hint,
                    width = area.width as usize - title.len()
                ),
                Style::default().bg(theme::BG_BAR).fg(theme::INFO),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(theme::BG_BAR)),
            area,
        );
    }

    fn render_help_bar(&self, frame: &mut Frame, area: Rect) {
        let hints = [
            theme::ICON_ARROW_LR,
            "调整",
            "Tab",
            "切换焦点",
            "r",
            "重新生成",
            "Enter",
            "复制",
            "p",
            "生成器",
            "Esc",
            "返回",
        ];
        let hint_text = hints.chunks(2).fold(String::new(), |mut acc, pair| {
            if !acc.is_empty() {
                acc.push_str(&format!("  {}  ", theme::ICON_PIPE));
            }
            acc.push_str(&format!("{} {}", pair[0], pair[1]));
            acc
        });
        let line = Line::from(Span::styled(
            format!(" {} ", hint_text),
            Style::default().fg(theme::TEXT_MUTED),
        ));
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(theme::BG_BAR)),
            area,
        );
    }
}

impl Default for PasswordGeneratorScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for PasswordGeneratorScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key) => self.handle_key(key, ctx),
            Message::CommandCompleted(result) => {
                match result {
                    crate::commands::result::CommandResult::CopiedToClipboard { .. } => {
                        self.hint_message = Some("已复制到剪贴板".to_string());
                    }
                    crate::commands::result::CommandResult::Error { fallback, .. } => {
                        self.hint_message = Some(fallback);
                    }
                    _ => {}
                }
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // title bar
                Constraint::Fill(1),   // generator content
                Constraint::Length(1), // help bar
            ])
            .split(area);

        self.render_title_bar(frame, chunks[0]);

        // Generator content panel
        let content_area = chunks[1];
        let width = content_area.width;
        let panel_lines = generator_panel::render_generator_panel(&self.state, false, width, true); // TODO: wire up unicode from AppState
        let paragraph = Paragraph::new(panel_lines);
        frame.render_widget(paragraph, content_area);

        self.render_help_bar(frame, chunks[2]);
    }

    fn on_mount(&mut self, ctx: &mut ScreenContext) {
        self.state = GeneratorState::from_config(
            Some(ctx.config.password.length),
            Some(ctx.config.password.include_uppercase),
            Some(ctx.config.password.include_digits),
            Some(ctx.config.password.include_special),
        );
        self.hint_message = None;
    }

    fn on_unmount(&mut self) {
        self.state.clear_preview();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_screen_has_sensible_defaults() {
        let screen = PasswordGeneratorScreen::new();
        assert_eq!(screen.state.style, GenerationStyle::Random);
        assert!(!screen.state.preview.is_empty());
        assert_eq!(screen.state.focus, GeneratorFocus::StyleSelector);
        assert!(screen.hint_message.is_none());
    }

    #[test]
    fn tab_cycles_focus() {
        let mut screen = PasswordGeneratorScreen::new();
        let initial = screen.state.focus;
        screen.state.focus_next();
        assert_ne!(screen.state.focus, initial);
    }

    #[test]
    fn esc_returns_pop_screen() {
        let mut screen = PasswordGeneratorScreen::new();
        let mut ctx = ScreenContext {
            command_tx: &tokio::sync::mpsc::channel(1).0,
            config: &Default::default(),
        };
        let result = screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Esc,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::PopScreen));
    }

    #[test]
    fn regenerate_key_works() {
        let mut screen = PasswordGeneratorScreen::new();
        let _before = screen.state.preview.clone();
        let mut ctx = ScreenContext {
            command_tx: &tokio::sync::mpsc::channel(1).0,
            config: &Default::default(),
        };
        // Press 'r' — should regenerate
        let result = screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Char('r'),
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        // Preview should be non-empty after regeneration
        assert!(!screen.state.preview.is_empty());
    }

    #[test]
    fn left_right_switches_style() {
        let mut screen = PasswordGeneratorScreen::new();
        screen.state.focus = GeneratorFocus::StyleSelector;

        // Right: Random -> Memorable
        let mut ctx = ScreenContext {
            command_tx: &tokio::sync::mpsc::channel(1).0,
            config: &Default::default(),
        };
        let result = screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Right,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.state.style, GenerationStyle::Memorable);

        // Right: Memorable -> Pin
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Right,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert_eq!(screen.state.style, GenerationStyle::Pin);

        // Left: Pin -> Memorable
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Left,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert_eq!(screen.state.style, GenerationStyle::Memorable);
    }

    #[test]
    fn length_slider_adjusts() {
        let mut screen = PasswordGeneratorScreen::new();
        screen.state.focus = GeneratorFocus::LengthSlider;
        screen.state.random_config.length = 20;
        let mut ctx = ScreenContext {
            command_tx: &tokio::sync::mpsc::channel(1).0,
            config: &Default::default(),
        };
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Left,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert_eq!(screen.state.random_config.length, 19);

        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Right,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert_eq!(screen.state.random_config.length, 20);
    }

    #[test]
    fn enter_on_action_copies_to_clipboard() {
        let mut screen = PasswordGeneratorScreen::new();
        screen.state.focus = GeneratorFocus::ActionButton;
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &Default::default(),
        };
        let result = screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.hint_message.is_some());
        // Command should be dispatched
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn enter_on_regenerate_regenerates() {
        let mut screen = PasswordGeneratorScreen::new();
        screen.state.focus = GeneratorFocus::RegenerateButton;
        let mut ctx = ScreenContext {
            command_tx: &tokio::sync::mpsc::channel(1).0,
            config: &Default::default(),
        };
        let result = screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert!(!screen.state.preview.is_empty());
    }

    #[test]
    fn toggle_char_type_works() {
        let mut screen = PasswordGeneratorScreen::new();
        screen.state.focus = GeneratorFocus::Toggle(0); // uppercase
        let mut ctx = ScreenContext {
            command_tx: &tokio::sync::mpsc::channel(1).0,
            config: &Default::default(),
        };
        let was_upper = screen.state.random_config.uppercase;
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Char(' '),
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert_eq!(screen.state.random_config.uppercase, !was_upper);
    }

    #[test]
    fn command_completed_clipboard_shows_hint() {
        let mut screen = PasswordGeneratorScreen::new();
        let mut ctx = ScreenContext {
            command_tx: &tokio::sync::mpsc::channel(1).0,
            config: &Default::default(),
        };
        let result = screen.update(
            Message::CommandCompleted(crate::commands::result::CommandResult::CopiedToClipboard {
                field: crate::commands::types::FieldSelector::Password,
                clear_after_seconds: 0,
            }),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.hint_message.as_deref(), Some("已复制到剪贴板"));
    }

    #[test]
    fn on_unmount_clears_preview() {
        let mut screen = PasswordGeneratorScreen::new();
        assert!(!screen.state.preview.is_empty());
        screen.on_unmount();
        assert!(screen.state.preview.is_empty());
    }

    #[test]
    fn enter_on_style_selector_does_not_copy() {
        let mut screen = PasswordGeneratorScreen::new();
        screen.state.focus = GeneratorFocus::StyleSelector;
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &Default::default(),
        };
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        // No command dispatched, no hint message
        assert!(rx.try_recv().is_err());
        assert!(screen.hint_message.is_none());
    }

    #[test]
    fn enter_on_length_slider_does_not_copy() {
        let mut screen = PasswordGeneratorScreen::new();
        screen.state.focus = GeneratorFocus::LengthSlider;
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &Default::default(),
        };
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(rx.try_recv().is_err());
        assert!(screen.hint_message.is_none());
    }

    #[test]
    fn enter_on_toggle_does_not_copy() {
        let mut screen = PasswordGeneratorScreen::new();
        screen.state.focus = GeneratorFocus::Toggle(0);
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &Default::default(),
        };
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(rx.try_recv().is_err());
        assert!(screen.hint_message.is_none());
    }

    #[test]
    fn enter_on_action_regenerates_preview_after_take() {
        let mut screen = PasswordGeneratorScreen::new();
        screen.state.focus = GeneratorFocus::ActionButton;
        let (tx, _) = tokio::sync::mpsc::channel(1);
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &Default::default(),
        };
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        // After copy, preview is regenerated (non-empty)
        assert!(!screen.state.preview.is_empty());
    }

    #[test]
    fn hint_persists_across_arrow_keypress() {
        let mut screen = PasswordGeneratorScreen::new();
        screen.state.focus = GeneratorFocus::ActionButton;
        let (tx, _) = tokio::sync::mpsc::channel(1);
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &Default::default(),
        };
        // Copy to set hint
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(screen.hint_message.is_some());
        // Arrow key does not clear hint (not a state-changing action)
        screen.state.focus = GeneratorFocus::StyleSelector;
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Down,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(screen.hint_message.is_some());
    }

    #[test]
    fn hint_cleared_on_style_change() {
        let mut screen = PasswordGeneratorScreen::new();
        screen.hint_message = Some("test hint".to_string());
        screen.state.focus = GeneratorFocus::StyleSelector;
        let mut ctx = ScreenContext {
            command_tx: &tokio::sync::mpsc::channel(1).0,
            config: &Default::default(),
        };
        // Right arrow changes style → clears hint
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Right,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(screen.hint_message.is_none());
    }
}
