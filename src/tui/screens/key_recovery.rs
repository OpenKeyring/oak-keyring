//! Key recovery screen — 24-word BIP39 recovery key input for partial vault recovery.
//!
//! Used when:
//! - Startup detects `no key + has db` (routed to KeyRecovery screen).
//! - Onboarding "Restore existing vault" is selected (step 1 of 3).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::commands::result::CommandResult;
use crate::commands::types::Screen;
use crate::commands::{Command, Message};
use crate::t;
use crate::tui::screens::onboarding::views_setup::{header_rows, render_header};
use crate::tui::screens::recovery_key::WordGridState;
use crate::tui::terminal::WidthTier;
use crate::tui::theme::{self, BRAND, ERROR, PRIMARY, TEXT, TEXT_MUTED};
use crate::tui::traits::screen::{Screen as ScreenTrait, ScreenContext, ScreenResult};
use zeroize::Zeroize;

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRecoveryOrigin {
    /// Startup: db exists but wrapped_secret_key.json is missing.
    StartupDbOnly,
    /// Onboarding: user selected "Restore existing vault".
    OnboardingRestore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyRecoveryFocus {
    #[default]
    Words,
    Reset,
    Confirm,
}

// ── KeyRecoveryScreen ─────────────────────────────────────────────────────

#[derive(Debug)]
pub struct KeyRecoveryScreen {
    pub origin: KeyRecoveryOrigin,
    pub words: WordGridState,
    pub error: Option<String>,
    pub validating: bool,
    pub focus: KeyRecoveryFocus,
}

impl Default for KeyRecoveryScreen {
    fn default() -> Self {
        Self::new(KeyRecoveryOrigin::StartupDbOnly)
    }
}

impl KeyRecoveryScreen {
    pub fn new(origin: KeyRecoveryOrigin) -> Self {
        Self {
            origin,
            words: WordGridState::default(),
            error: None,
            validating: false,
            focus: KeyRecoveryFocus::Words,
        }
    }

    fn step_text(&self) -> std::borrow::Cow<'static, str> {
        match self.origin {
            KeyRecoveryOrigin::StartupDbOnly => t!("tui.entry.key_recovery_step_1_2"),
            KeyRecoveryOrigin::OnboardingRestore => t!("tui.entry.key_recovery_step_1_3"),
        }
    }

    fn submit_recovery_words(&mut self, ctx: &mut ScreenContext) {
        if self.words.all_filled() {
            self.error = None;
            self.validating = true;
            match self.words.collect_recovery_words() {
                Ok(words) => {
                    if ctx
                        .command_tx
                        .try_send(Command::ValidateRecoveryWords { words })
                        .is_err()
                    {
                        self.validating = false;
                        self.error = Some(t!("tui.error.command_dispatch_failed").to_string());
                    }
                }
                Err(_) => {
                    self.validating = false;
                    self.error = Some(t!("tui.entry.key_recovery_empty_error").to_string());
                }
            }
        } else {
            self.error = Some(t!("tui.entry.key_recovery_empty_error").to_string());
        }
    }

    fn reset_recovery_words(&mut self) {
        for word in &mut self.words.words {
            word.zeroize();
            word.clear();
        }
        self.words.errors = [false; 24];
        self.words.focused_index = 0;
        self.error = None;
        self.validating = false;
        self.focus = KeyRecoveryFocus::Words;
    }

    fn focus_next(&mut self) {
        match self.focus {
            KeyRecoveryFocus::Words if self.words.focused_index == 23 => {
                self.focus = KeyRecoveryFocus::Reset;
            }
            KeyRecoveryFocus::Words => self.words.next_word(),
            KeyRecoveryFocus::Reset => self.focus = KeyRecoveryFocus::Confirm,
            KeyRecoveryFocus::Confirm => {
                self.focus = KeyRecoveryFocus::Words;
                self.words.focused_index = 0;
            }
        }
    }

    fn focus_prev(&mut self) {
        match self.focus {
            KeyRecoveryFocus::Words if self.words.focused_index == 0 => {
                self.focus = KeyRecoveryFocus::Confirm;
            }
            KeyRecoveryFocus::Words => self.words.prev_word(),
            KeyRecoveryFocus::Reset => {
                self.focus = KeyRecoveryFocus::Words;
                self.words.focused_index = 23;
            }
            KeyRecoveryFocus::Confirm => self.focus = KeyRecoveryFocus::Reset,
        }
    }

    fn handle_navigation_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => self.focus_next(),
            KeyCode::BackTab => self.focus_prev(),
            KeyCode::Down
                if self.focus == KeyRecoveryFocus::Words && self.words.focused_index >= 20 =>
            {
                self.focus = if self.words.focused_index % 4 < 2 {
                    KeyRecoveryFocus::Reset
                } else {
                    KeyRecoveryFocus::Confirm
                };
            }
            KeyCode::Up if self.focus == KeyRecoveryFocus::Reset => {
                self.focus = KeyRecoveryFocus::Words;
                self.words.focused_index = 20;
            }
            KeyCode::Up if self.focus == KeyRecoveryFocus::Confirm => {
                self.focus = KeyRecoveryFocus::Words;
                self.words.focused_index = 23;
            }
            KeyCode::Left if self.focus == KeyRecoveryFocus::Confirm => {
                self.focus = KeyRecoveryFocus::Reset;
            }
            KeyCode::Right if self.focus == KeyRecoveryFocus::Reset => {
                self.focus = KeyRecoveryFocus::Confirm;
            }
            _ if self.focus == KeyRecoveryFocus::Words => {
                self.words.handle_key(key);
            }
            _ => {}
        }
    }

    fn handle_key_inner(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Esc => ScreenResult::PopScreen,
            KeyCode::Enter => {
                match self.focus {
                    KeyRecoveryFocus::Words => {
                        if self.words.all_filled() {
                            self.focus = KeyRecoveryFocus::Confirm;
                            self.error = None;
                        } else {
                            self.words.next_word();
                        }
                    }
                    KeyRecoveryFocus::Reset => self.reset_recovery_words(),
                    KeyRecoveryFocus::Confirm => self.submit_recovery_words(ctx),
                }
                ScreenResult::Continue
            }
            _ => {
                self.handle_navigation_key(key);
                ScreenResult::Continue
            }
        }
    }

    #[cfg(test)]
    pub fn handle_key_for_test(&mut self, key: KeyEvent) -> ScreenResult {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let config = crate::config::AppConfig::default();
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &config,
        };
        self.handle_key_inner(key, &mut ctx)
    }
}

impl ScreenTrait for KeyRecoveryScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key)
                if key.kind == KeyEventKind::Press && key.modifiers.is_empty()
                    || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.handle_key_inner(key, ctx)
            }
            Message::CommandCompleted(CommandResult::RecoveryWordsValidated) => {
                self.validating = false;
                self.error = None;
                ScreenResult::NavigateTo(Screen::SetNewMasterPassword)
            }
            Message::CommandCompleted(CommandResult::Error { fallback, .. }) => {
                self.validating = false;
                self.error = Some(fallback);
                ScreenResult::Continue
            }
            Message::CommandCompleted(CommandResult::Cancelled { .. }) => {
                self.validating = false;
                self.error = None;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let wide = WidthTier::from_width(area.width) != WidthTier::TooSmall;
        let use_onboarding_header = matches!(self.origin, KeyRecoveryOrigin::OnboardingRestore);
        let brand_rows = if use_onboarding_header {
            header_rows(wide)
        } else {
            1
        };
        let separator_rows = if use_onboarding_header { 0 } else { 1 };
        let content_area = Self::centered_content(area, 17 + brand_rows + separator_rows);

        let rows = Layout::vertical([
            Constraint::Length(brand_rows),     // logo or brand
            Constraint::Length(separator_rows), // separator for compact startup recovery
            Constraint::Length(2),              // title
            Constraint::Length(2),              // instruction
            Constraint::Length(8),              // 24-word grid
            Constraint::Length(1),              // reset/confirm actions
            Constraint::Length(1),              // error or hint
            Constraint::Length(1),              // hotkey
            Constraint::Length(1),              // step
        ])
        .split(content_area);

        if use_onboarding_header {
            render_header(frame, rows[0], wide);
        } else {
            // Brand
            let brand = Paragraph::new(Line::from(vec![
                Span::styled(format!("{} ", theme::ICON_LOCK), Style::default().fg(BRAND)),
                Span::styled(
                    "OpenKeyring",
                    Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
                ),
            ]))
            .alignment(Alignment::Center);
            frame.render_widget(brand, rows[0]);

            // Separator
            let sep = Paragraph::new(Line::from(Span::styled(
                "─────────────────────────────",
                Style::default().fg(TEXT_MUTED),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(sep, rows[1]);
        }

        // Title
        let title = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.key_recovery_title"),
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        // Instruction
        let instruction = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.key_recovery_instruction"),
            Style::default().fg(TEXT),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[3]);

        // 24-word grid
        let grid_area = rows[4];
        self.words.view(frame, grid_area);

        self.render_actions(frame, rows[5]);

        // Error or hint
        let idx = 6;
        if self.validating {
            let validating_line = Paragraph::new(Line::from(Span::styled(
                format!(
                    "{} {}",
                    theme::ICON_SYNC_SYNCING,
                    t!("tui.entry.key_recovery_validating")
                ),
                Style::default().fg(PRIMARY),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(validating_line, rows[idx]);
        } else if let Some(ref err) = self.error {
            let error_line = Paragraph::new(Line::from(Span::styled(
                format!("✕ {}", err),
                Style::default().fg(ERROR),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(error_line, rows[idx]);
        }

        // Hotkeys
        let hotkey = if self.validating {
            t!("tui.entry.key_recovery_hotkey_validating")
        } else if self.error.is_some() {
            t!("tui.entry.key_recovery_hotkey_error")
        } else {
            t!("tui.entry.key_recovery_hotkey")
        };
        let hotkey_para = Paragraph::new(Line::from(Span::styled(
            hotkey,
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey_para, rows[7]);

        // Step indicator
        let step_text = self.step_text();
        let step = Paragraph::new(Line::from(Span::styled(
            step_text.as_ref(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[8]);
    }

    fn on_mount(&mut self, _ctx: &mut ScreenContext) {}

    fn on_unmount(&mut self) {
        use zeroize::Zeroize;
        self.words.zeroize();
    }
}

impl KeyRecoveryScreen {
    fn centered_content(area: ratatui::layout::Rect, content_height: u16) -> ratatui::layout::Rect {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(content_height),
            Constraint::Fill(1),
        ])
        .split(area);

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(72),
            Constraint::Fill(1),
        ])
        .split(outer[1]);

        h_layout[1]
    }

    fn button_style(&self, focus: KeyRecoveryFocus, primary: bool) -> Style {
        let base = if primary {
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_MUTED)
        };
        if self.focus == focus {
            base.add_modifier(Modifier::REVERSED)
        } else {
            base
        }
    }

    fn render_actions(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let actions = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("[ {} ]", t!("tui.entry.key_recovery_reset_button")),
                self.button_style(KeyRecoveryFocus::Reset, false),
            ),
            Span::raw("  "),
            Span::styled(
                format!("[ {} ]", t!("tui.entry.key_recovery_confirm_button")),
                self.button_style(KeyRecoveryFocus::Confirm, true),
            ),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(actions, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::traits::screen::Screen as ScreenTrait;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    fn render_key_recovery_buffer(
        screen: &KeyRecoveryScreen,
        width: u16,
        height: u16,
    ) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                screen.view(frame, frame.area());
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn key_recovery_starts_with_empty_24_word_grid() {
        let screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::StartupDbOnly);
        assert_eq!(screen.origin, KeyRecoveryOrigin::StartupDbOnly);
        assert_eq!(screen.words.words.len(), 24);
        assert_eq!(screen.error, None);
    }

    #[test]
    fn onboarding_restore_renders_ascii_logo_on_tall_terminal() {
        let screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::OnboardingRestore);

        let buffer = render_key_recovery_buffer(&screen, 80, 24);

        assert!(format!("{buffer:?}").contains("░█▀█"));
    }

    #[test]
    fn confirm_with_incomplete_words_sets_inline_error() {
        let mut screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::OnboardingRestore);
        screen.focus = KeyRecoveryFocus::Confirm;
        let result = screen.handle_key_for_test(key(KeyCode::Enter));
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(
            screen.error.as_deref(),
            Some(t!("tui.entry.key_recovery_empty_error").as_ref())
        );
    }

    #[test]
    fn enter_on_filled_words_does_not_submit_without_confirm_button() {
        let mut screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::OnboardingRestore);
        for word in &mut screen.words.words {
            word.push_str("abandon");
        }
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let config = crate::config::AppConfig::default();
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &config,
        };

        let result = screen.handle_key_inner(key(KeyCode::Enter), &mut ctx);

        assert!(matches!(result, ScreenResult::Continue));
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        assert!(!screen.validating);
        assert!(screen.error.is_none());
    }

    #[test]
    fn recovery_actions_are_rendered() {
        let screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::StartupDbOnly);

        let buffer = render_key_recovery_buffer(&screen, 80, 24);
        let rendered = format!("{buffer:?}");

        assert!(rendered.contains("Reset"));
        assert!(rendered.contains("Confirm"));
    }

    #[test]
    fn tab_moves_from_last_word_to_reset_then_confirm() {
        let mut screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::StartupDbOnly);
        screen.words.focused_index = 23;

        screen.handle_key_for_test(key(KeyCode::Tab));
        assert_eq!(screen.focus, KeyRecoveryFocus::Reset);

        screen.handle_key_for_test(key(KeyCode::Tab));
        assert_eq!(screen.focus, KeyRecoveryFocus::Confirm);
    }

    #[test]
    fn reset_button_clears_words_and_errors() {
        let mut screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::StartupDbOnly);
        for word in &mut screen.words.words {
            word.push_str("abandon");
        }
        screen.words.errors[3] = true;
        screen.error = Some("bad words".to_string());
        screen.focus = KeyRecoveryFocus::Reset;

        screen.handle_key_for_test(key(KeyCode::Enter));

        assert!(screen.words.words.iter().all(String::is_empty));
        assert!(screen.words.errors.iter().all(|error| !error));
        assert_eq!(screen.words.focused_index, 0);
        assert_eq!(screen.focus, KeyRecoveryFocus::Words);
        assert!(screen.error.is_none());
    }

    #[test]
    fn confirm_button_submits_filled_words() {
        let mut screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::StartupDbOnly);
        for word in &mut screen.words.words {
            word.push_str("abandon");
        }
        screen.focus = KeyRecoveryFocus::Confirm;
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let config = crate::config::AppConfig::default();
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &config,
        };

        let result = screen.handle_key_inner(key(KeyCode::Enter), &mut ctx);

        assert!(matches!(result, ScreenResult::Continue));
        assert!(matches!(
            rx.try_recv(),
            Ok(Command::ValidateRecoveryWords { .. })
        ));
        assert!(screen.validating);
        assert!(screen.error.is_none());
    }
}
