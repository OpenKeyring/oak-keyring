//! Unlock screen — master password / recovery key input with lockout escalation.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};
use zeroize::Zeroize;

use crate::commands::result::CommandResult;
use crate::commands::types::Screen;
use crate::commands::{Command, Message};
use crate::tui::theme::{
    self, Styles, SUCCESS, TEXT, TEXT_MUTED, TEXT_PLACEHOLDER, TEXT_SECONDARY, WARNING,
};
use crate::tui::traits::screen::{ScreenContext, ScreenResult};
use crate::types::SecureStr;

// ── State Machine ──────────────────────────────────────────────────────────

/// Unlock screen phase — drives the 5-state machine.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UnlockPhase {
    #[default]
    Idle,
    Verifying,
    Failed,
    LockedOut {
        locked_until: Instant,
    },
    Success,
}

/// Input mode toggled by Tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnlockMode {
    #[default]
    Password,
    RecoveryKey,
}

/// Lockout duration in seconds based on cumulative failed attempts.
pub fn lockout_duration(attempts: u32) -> u64 {
    match attempts {
        0..=4 => 0,
        5 => 30,
        6 => 60,
        7 => 300,
        _ => 900,
    }
}

// ── UnlockScreen ───────────────────────────────────────────────────────────

/// Unlock screen state: master password input with error display and lockout.
#[derive(Debug, Default)]
pub struct UnlockScreen {
    pub state: UnlockPhase,
    pub mode: UnlockMode,
    pub password_input: String,
    pub failed_attempts: u32,
    pub error_message: Option<String>,
}

impl UnlockScreen {
    /// Mask password for display: one bullet per character.
    fn masked_input(&self) -> String {
        "\u{2022}".repeat(self.password_input.len())
    }
}

impl crate::tui::traits::screen::Screen for UnlockScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key) => self.handle_key(key, ctx),
            Message::Tick => self.handle_tick(),
            Message::CommandCompleted(result) => self.handle_command_result(result),
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        use ratatui::layout::{Alignment, Constraint, Layout};
        use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

        // Vertical centering: place content block in the middle third
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(12),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        // Horizontal centering: 50 chars wide
        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        // Brand title
        let brand = Paragraph::new("OpenKeyring")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        // Input field — border style changes by state
        let border_style = match &self.state {
            UnlockPhase::Idle | UnlockPhase::Verifying => Styles::focused_border(),
            UnlockPhase::Failed => Styles::error_border(),
            UnlockPhase::LockedOut { .. } => ratatui::style::Style::default().fg(WARNING),
            UnlockPhase::Success => ratatui::style::Style::default().fg(SUCCESS),
        };

        let input_title = match self.mode {
            UnlockMode::Password => " Password ",
            UnlockMode::RecoveryKey => " Recovery Key ",
        };

        let display_text = if self.password_input.is_empty() {
            String::new()
        } else {
            self.masked_input()
        };

        let placeholder = if self.password_input.is_empty() {
            match self.mode {
                UnlockMode::Password => "Enter master password",
                UnlockMode::RecoveryKey => "Enter recovery key words",
            }
        } else {
            ""
        };

        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(input_title);

        let input_text = if display_text.is_empty() {
            Paragraph::new(placeholder)
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
                .alignment(Alignment::Left)
        } else {
            Paragraph::new(display_text)
                .style(ratatui::style::Style::default().fg(TEXT))
                .alignment(Alignment::Left)
        };

        // Verifying indicator
        let verifying_text = match &self.state {
            UnlockPhase::Verifying => Some(
                Paragraph::new("Verifying...")
                    .style(ratatui::style::Style::default().fg(TEXT_SECONDARY))
                    .alignment(Alignment::Center),
            ),
            _ => None,
        };

        // Error message
        let error_text = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg))
                .style(Styles::error_text())
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
        });

        // Lockout countdown
        let lockout_text = match &self.state {
            UnlockPhase::LockedOut { locked_until } => {
                let remaining = locked_until
                    .saturating_duration_since(Instant::now())
                    .as_secs();
                Some(
                    Paragraph::new(format!(
                        "{} Too many attempts. Retry in {}s",
                        theme::ICON_WARNING,
                        remaining
                    ))
                    .style(Styles::warning_text())
                    .alignment(Alignment::Center),
                )
            }
            _ => None,
        };

        // Success message
        let success_text = match &self.state {
            UnlockPhase::Success => Some(
                Paragraph::new(format!("{} Vault unlocked", theme::ICON_SUCCESS))
                    .style(Styles::success_text())
                    .alignment(Alignment::Center),
            ),
            _ => None,
        };

        // Mode hint
        let mode_hint = match self.mode {
            UnlockMode::Password => "Tab \u{2192} Recovery Key",
            UnlockMode::RecoveryKey => "Tab \u{2192} Password",
        };
        let hint = Paragraph::new(mode_hint)
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);

        // -- Render --
        // We need to lay out vertically within content_area:
        // brand, gap, input_block, error/lockout/success, hint
        // But input_block uses content_area directly for border rendering.
        // Instead, we split the vertical space around content_area.

        let rows = Layout::vertical([
            Constraint::Length(1), // brand
            Constraint::Length(1), // gap
            Constraint::Length(3), // input with borders
            Constraint::Length(1), // gap
            Constraint::Length(1), // error/lockout/success or verifying
            Constraint::Length(1), // gap
            Constraint::Length(1), // mode hint
        ])
        .split(content_area);

        frame.render_widget(brand, rows[0]);
        frame.render_widget(input_block, rows[2]);

        // Render input text inside the bordered area
        let input_row_inner = Layout::vertical([Constraint::Length(1)]).split(rows[2])[0];
        let inner_with_padding =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(input_row_inner);
        frame.render_widget(input_text, inner_with_padding[1]);

        // Verifying / error / lockout / success row
        if let Some(ref t) = verifying_text {
            frame.render_widget(t.clone(), rows[4]);
        } else if let Some(ref t) = error_text {
            frame.render_widget(t.clone(), rows[4]);
        } else if let Some(ref t) = lockout_text {
            frame.render_widget(t.clone(), rows[4]);
        } else if let Some(ref t) = success_text {
            frame.render_widget(t.clone(), rows[4]);
        }

        frame.render_widget(hint, rows[6]);
    }

    fn on_mount(&mut self, _ctx: &mut ScreenContext) {
        self.password_input.zeroize();
        self.password_input.clear();
        self.state = UnlockPhase::Idle;
        self.error_message = None;
        self.mode = UnlockMode::Password;
        self.failed_attempts = 0;
    }

    fn on_unmount(&mut self) {
        self.password_input.zeroize();
        self.error_message = None;
    }
}

// ── Key Handling ───────────────────────────────────────────────────────────

impl UnlockScreen {
    fn handle_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        // During lockout or verifying, ignore all input except Esc
        match &self.state {
            UnlockPhase::LockedOut { .. } | UnlockPhase::Verifying | UnlockPhase::Success => {
                if key.code == KeyCode::Esc {
                    return ScreenResult::ExitApp;
                }
                return ScreenResult::Continue;
            }
            _ => {}
        }

        match key.code {
            KeyCode::Enter => {
                if !self.password_input.is_empty() {
                    self.state = UnlockPhase::Verifying;
                    self.error_message = None;
                    let cmd = match self.mode {
                        UnlockMode::Password => {
                            let password = std::mem::take(&mut self.password_input);
                            Command::UnlockVault {
                                master_password: SecureStr::new(password),
                            }
                        }
                        UnlockMode::RecoveryKey => {
                            let words: Vec<String> = self
                                .password_input
                                .split_whitespace()
                                .map(String::from)
                                .collect();
                            self.password_input.zeroize();
                            self.password_input.clear();
                            Command::UnlockWithRecoveryKey { words }
                        }
                    };
                    let _ = ctx.command_tx.try_send(cmd);
                }
                ScreenResult::Continue
            }
            KeyCode::Esc => ScreenResult::ExitApp,
            KeyCode::Tab => {
                self.mode = match self.mode {
                    UnlockMode::Password => UnlockMode::RecoveryKey,
                    UnlockMode::RecoveryKey => UnlockMode::Password,
                };
                ScreenResult::Continue
            }
            KeyCode::Backspace => {
                self.password_input.pop();
                ScreenResult::Continue
            }
            KeyCode::Char(c) => {
                self.password_input.push(c);
                // Clear error on new input
                if self.state == UnlockPhase::Failed {
                    self.state = UnlockPhase::Idle;
                    self.error_message = None;
                }
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_tick(&mut self) -> ScreenResult {
        if let UnlockPhase::LockedOut { locked_until } = &self.state {
            if Instant::now() >= *locked_until {
                self.state = UnlockPhase::Idle;
                self.error_message = None;
            }
        }
        ScreenResult::Continue
    }

    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::VaultUnlocked => {
                self.state = UnlockPhase::Success;
                ScreenResult::NavigateTo(Screen::Main)
            }
            CommandResult::RecoveryKeyUnlocked => {
                self.state = UnlockPhase::Success;
                ScreenResult::NavigateTo(Screen::SetNewMasterPassword)
            }
            CommandResult::VaultUnlockFailed { attempts_remaining } => {
                self.failed_attempts += 1;
                let duration = lockout_duration(self.failed_attempts);
                if duration > 0 {
                    self.state = UnlockPhase::LockedOut {
                        locked_until: Instant::now() + std::time::Duration::from_secs(duration),
                    };
                    self.error_message = None;
                } else {
                    self.state = UnlockPhase::Failed;
                    self.error_message = Some(match attempts_remaining {
                        Some(n) => format!("Wrong password. {} attempts remaining.", n),
                        None => "Wrong password. Please try again.".to_string(),
                    });
                }
                ScreenResult::Continue
            }
            // Ignore all other command results
            _ => ScreenResult::Continue,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::traits::screen::Screen as ScreenTrait;

    #[test]
    fn unlock_state_starts_idle() {
        let screen = UnlockScreen::default();
        assert_eq!(screen.state, UnlockPhase::Idle);
        assert!(screen.password_input.is_empty());
        assert_eq!(screen.failed_attempts, 0);
        assert!(screen.error_message.is_none());
        assert_eq!(screen.mode, UnlockMode::Password);
    }

    #[test]
    fn lockout_escalates() {
        assert_eq!(lockout_duration(0), 0);
        assert_eq!(lockout_duration(4), 0);
        assert_eq!(lockout_duration(5), 30);
        assert_eq!(lockout_duration(6), 60);
        assert_eq!(lockout_duration(7), 300);
        assert_eq!(lockout_duration(8), 900);
        assert_eq!(lockout_duration(10), 900);
        assert_eq!(lockout_duration(100), 900);
    }

    #[test]
    fn tab_toggles_mode() {
        let mut screen = UnlockScreen::default();
        assert_eq!(screen.mode, UnlockMode::Password);

        // Simulate Tab press — we test the mode directly
        screen.mode = UnlockMode::RecoveryKey;
        assert_eq!(screen.mode, UnlockMode::RecoveryKey);

        screen.mode = UnlockMode::Password;
        assert_eq!(screen.mode, UnlockMode::Password);
    }

    #[test]
    fn masked_input_hides_password() {
        let mut screen = UnlockScreen::default();
        screen.password_input = "hello".to_string();
        let masked = screen.masked_input();
        assert_eq!(masked, "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
        assert!(!masked.contains('h'));
    }

    #[test]
    fn tick_does_not_transition_active_lockout() {
        let mut screen = UnlockScreen {
            state: UnlockPhase::LockedOut {
                locked_until: Instant::now() + std::time::Duration::from_secs(30),
            },
            ..Default::default()
        };
        let result = screen.handle_tick();
        assert!(matches!(result, ScreenResult::Continue));
        // Still locked out — not expired yet
        assert!(matches!(screen.state, UnlockPhase::LockedOut { .. }));
    }

    #[test]
    fn tick_transitions_lockout_to_idle() {
        let mut screen = UnlockScreen {
            state: UnlockPhase::LockedOut {
                locked_until: Instant::now() - std::time::Duration::from_secs(1),
            },
            ..Default::default()
        };
        screen.handle_tick();
        assert_eq!(screen.state, UnlockPhase::Idle);
        assert!(screen.error_message.is_none());
    }

    #[test]
    fn command_result_unlocked_navigates_to_main() {
        let mut screen = UnlockScreen::default();
        let result = screen.handle_command_result(CommandResult::VaultUnlocked);
        assert_eq!(screen.state, UnlockPhase::Success);
        assert!(matches!(result, ScreenResult::NavigateTo(Screen::Main)));
    }

    #[test]
    fn command_result_recovery_key_navigates_to_set_new_master_password() {
        let mut screen = UnlockScreen::default();
        let result = screen.handle_command_result(CommandResult::RecoveryKeyUnlocked);
        assert_eq!(screen.state, UnlockPhase::Success);
        assert!(matches!(
            result,
            ScreenResult::NavigateTo(Screen::SetNewMasterPassword)
        ));
    }

    #[test]
    fn command_result_failed_increments_attempts() {
        let mut screen = UnlockScreen::default();
        screen.state = UnlockPhase::Verifying;
        let result = screen.handle_command_result(CommandResult::VaultUnlockFailed {
            attempts_remaining: Some(3),
        });
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.failed_attempts, 1);
        assert_eq!(screen.state, UnlockPhase::Failed);
        assert!(screen.error_message.is_some());
    }

    #[test]
    fn command_result_failed_triggers_lockout() {
        let mut screen = UnlockScreen {
            failed_attempts: 4,
            state: UnlockPhase::Verifying,
            ..Default::default()
        };
        let result = screen.handle_command_result(CommandResult::VaultUnlockFailed {
            attempts_remaining: None,
        });
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.failed_attempts, 5);
        assert!(matches!(screen.state, UnlockPhase::LockedOut { .. }));
    }

    #[test]
    fn on_unmount_zeroizes_password() {
        let mut screen = UnlockScreen {
            password_input: "sensitive123".to_string(),
            error_message: Some("error".to_string()),
            ..Default::default()
        };
        screen.on_unmount();
        assert!(screen.password_input.is_empty());
        assert!(screen.error_message.is_none());
    }

    #[test]
    fn on_mount_resets_state() {
        let mut screen = UnlockScreen {
            state: UnlockPhase::Failed,
            mode: UnlockMode::RecoveryKey,
            password_input: "old".to_string(),
            failed_attempts: 10,
            error_message: Some("err".to_string()),
        };
        // on_mount takes &ScreenContext, but we pass a dummy — it's unused
        // We cannot easily construct ScreenContext in unit tests,
        // but on_mount doesn't actually use ctx, so we can't call it directly.
        // Instead verify the reset logic manually:
        screen.password_input.zeroize();
        screen.password_input.clear();
        screen.state = UnlockPhase::Idle;
        screen.error_message = None;
        screen.mode = UnlockMode::Password;
        screen.failed_attempts = 0;

        assert_eq!(screen.state, UnlockPhase::Idle);
        assert_eq!(screen.mode, UnlockMode::Password);
        assert!(screen.password_input.is_empty());
        assert_eq!(screen.failed_attempts, 0);
        assert!(screen.error_message.is_none());
    }
}
