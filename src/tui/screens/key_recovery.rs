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
use crate::tui::screens::recovery_key::WordGridState;
use crate::tui::theme::{self, BRAND, ERROR, PRIMARY, TEXT, TEXT_MUTED};
use crate::tui::traits::screen::{Screen as ScreenTrait, ScreenContext, ScreenResult};

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRecoveryOrigin {
    /// Startup: db exists but wrapped_secret_key.json is missing.
    StartupDbOnly,
    /// Onboarding: user selected "Restore existing vault".
    OnboardingRestore,
}

// ── KeyRecoveryScreen ─────────────────────────────────────────────────────

#[derive(Debug)]
pub struct KeyRecoveryScreen {
    pub origin: KeyRecoveryOrigin,
    pub words: WordGridState,
    pub error: Option<String>,
    pub validating: bool,
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
        }
    }

    fn step_text(&self) -> std::borrow::Cow<'static, str> {
        match self.origin {
            KeyRecoveryOrigin::StartupDbOnly => t!("tui.entry.key_recovery_step_1_2"),
            KeyRecoveryOrigin::OnboardingRestore => t!("tui.entry.key_recovery_step_1_3"),
        }
    }

    fn handle_key_inner(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Esc => ScreenResult::PopScreen,
            KeyCode::Enter => {
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
                                self.error =
                                    Some(t!("tui.error.command_dispatch_failed").to_string());
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
                ScreenResult::Continue
            }
            _ => {
                self.words.handle_key(key);
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
        let content_area = Self::centered_content(area, 17);

        let rows = Layout::vertical([
            Constraint::Length(1), // brand
            Constraint::Length(1), // separator
            Constraint::Length(2), // title
            Constraint::Length(2), // instruction
            Constraint::Length(6), // 24-word grid
            Constraint::Length(1), // gap
            Constraint::Length(1), // error or hint
            Constraint::Length(1), // hotkey
            Constraint::Length(1), // step
        ])
        .split(content_area);

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

        // Error or hint
        let idx = 6;
        if let Some(ref err) = self.error {
            let error_line = Paragraph::new(Line::from(Span::styled(
                format!("✕ {}", err),
                Style::default().fg(ERROR),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(error_line, rows[idx]);
        }

        // Hotkeys
        let hotkey = if self.error.is_some() {
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
            Constraint::Max(60),
            Constraint::Fill(1),
        ])
        .split(outer[1]);

        h_layout[1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn key_recovery_starts_with_empty_24_word_grid() {
        let screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::StartupDbOnly);
        assert_eq!(screen.origin, KeyRecoveryOrigin::StartupDbOnly);
        assert_eq!(screen.words.words.len(), 24);
        assert_eq!(screen.error, None);
    }

    #[test]
    fn enter_with_incomplete_words_sets_inline_error() {
        let mut screen = KeyRecoveryScreen::new(KeyRecoveryOrigin::OnboardingRestore);
        let result = screen.handle_key_for_test(key(KeyCode::Enter));
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(
            screen.error.as_deref(),
            Some(t!("tui.entry.key_recovery_empty_error").as_ref())
        );
    }
}
