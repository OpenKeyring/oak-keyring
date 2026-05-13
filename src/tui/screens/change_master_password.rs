//! Change master password screen — two-step flow: verify current, then set new.

use crossterm::event::{KeyCode, KeyEvent};

use crate::commands::result::CommandResult;
use crate::commands::{Command, Message};
use crate::crypto::strength::{evaluate_strength, PasswordStrength, StrengthLevel};
use crate::t;
use crate::tui::theme::{
    self, Styles, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED, TEXT_PLACEHOLDER, WARNING,
};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::sensitive::SensitiveInput;

// ── Enums ───────────────────────────────────────────────────────────────────

/// Which password field is currently focused in step 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordField {
    New,
    Confirm,
}

// ── ChangeMasterPasswordScreen ────────────────────────────────────────────────

/// Two-step master password change screen.
///
/// Step 1: verify current password
/// Step 2: enter new password (with strength indicator) + confirm
#[derive(Debug)]
pub struct ChangeMasterPasswordScreen {
    pub step: u8,
    pub current_password: SensitiveInput,
    pub new_password: SensitiveInput,
    pub confirm_password: SensitiveInput,
    pending_verification: bool,
    pub focused: PasswordField,
    pub error_message: Option<String>,
    pub password_strength: Option<PasswordStrength>,
}

impl ChangeMasterPasswordScreen {
    pub fn new() -> Self {
        Self {
            step: 1,
            current_password: SensitiveInput::new(),
            new_password: SensitiveInput::new(),
            confirm_password: SensitiveInput::new(),
            pending_verification: false,
            focused: PasswordField::New,
            error_message: None,
            password_strength: None,
        }
    }

    /// Re-evaluate password strength from the new password field.
    fn update_strength(&mut self) {
        if self.new_password.is_empty() {
            self.password_strength = None;
        } else {
            self.new_password.expose(|s| {
                self.password_strength = Some(evaluate_strength(s));
            });
        }
    }

    /// Return masked display for a password string.
    fn display_password(password: &str) -> String {
        theme::ICON_PASSWORD_MASK.repeat(password.chars().count())
    }

    /// Map strength level to a theme color.
    fn strength_color(level: &StrengthLevel) -> ratatui::style::Color {
        match level {
            StrengthLevel::VeryWeak | StrengthLevel::Weak => ERROR,
            StrengthLevel::Fair => WARNING,
            StrengthLevel::Strong => PRIMARY,
            StrengthLevel::VeryStrong => SUCCESS,
        }
    }
}

impl Default for ChangeMasterPasswordScreen {
    fn default() -> Self {
        Self::new()
    }
}

// ── Screen trait impl ────────────────────────────────────────────────────────

impl Screen for ChangeMasterPasswordScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key) => self.handle_key(key, ctx),
            Message::CommandCompleted(result) => self.handle_command_result(result),
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        use ratatui::layout::{Alignment, Constraint, Layout};
        use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

        if self.step == 1 {
            self.view_step1(frame, area);
        } else {
            // Vertical centering
            let outer = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(20),
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

            // Title
            let title = Paragraph::new(t!("tui.entry.set_password_title"))
                .style(Styles::brand_text())
                .alignment(Alignment::Center);

            // -- New password field --
            let new_border_style = if self.focused == PasswordField::New {
                Styles::focused_border()
            } else {
                Styles::unfocused_border()
            };

            let new_display = if self.new_password.is_empty() {
                String::new()
            } else {
                self.new_password.expose(Self::display_password)
            };
            let new_placeholder = if self.new_password.is_empty() {
                t!("tui.entry.new_password_placeholder")
            } else {
                std::borrow::Cow::Borrowed("")
            };

            let new_input_block = Block::default()
                .borders(Borders::ALL)
                .border_style(new_border_style)
                .title(" New Password ");

            let new_input_text = if new_display.is_empty() {
                Paragraph::new(new_placeholder)
                    .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
            } else {
                Paragraph::new(new_display).style(ratatui::style::Style::default().fg(TEXT))
            };

            // -- Strength bar --
            let strength_line = if let Some(ref s) = self.password_strength {
                let bar_total = 16u8;
                let filled = s.bar_fill.min(bar_total);
                let empty = bar_total - filled;
                let bar_str = format!(
                    "{}{}",
                    theme::ICON_PROGRESS_FILL.repeat(filled as usize),
                    theme::ICON_PROGRESS_EMPTY.repeat(empty as usize)
                );
                let label = format!(
                    "{} {} {}",
                    t!("tui.entry.strength_label"),
                    match s.level {
                        crate::crypto::strength::StrengthLevel::VeryWeak => {
                            t!("tui.generator.strength_too_weak")
                        }
                        crate::crypto::strength::StrengthLevel::Weak => {
                            t!("tui.generator.strength_weak")
                        }
                        crate::crypto::strength::StrengthLevel::Fair => {
                            t!("tui.generator.strength_fair")
                        }
                        crate::crypto::strength::StrengthLevel::Strong => {
                            t!("tui.generator.strength_strong")
                        }
                        crate::crypto::strength::StrengthLevel::VeryStrong => {
                            t!("tui.generator.strength_very_strong")
                        }
                    },
                    bar_str
                );
                let color = Self::strength_color(&s.level);
                Paragraph::new(label).style(ratatui::style::Style::default().fg(color))
            } else {
                Paragraph::new(t!("tui.entry.strength_label"))
                    .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            };

            // -- Confirm password field --
            let confirm_border_style = if self.focused == PasswordField::Confirm {
                Styles::focused_border()
            } else {
                Styles::unfocused_border()
            };

            let confirm_display = if self.confirm_password.is_empty() {
                String::new()
            } else {
                self.confirm_password.expose(Self::display_password)
            };
            let confirm_placeholder = if self.confirm_password.is_empty() {
                t!("tui.entry.confirm_new_placeholder")
            } else {
                std::borrow::Cow::Borrowed("")
            };

            let confirm_input_block = Block::default()
                .borders(Borders::ALL)
                .border_style(confirm_border_style)
                .title(" Confirm Password ");

            let confirm_input_text = if confirm_display.is_empty() {
                Paragraph::new(confirm_placeholder)
                    .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
            } else {
                Paragraph::new(confirm_display).style(ratatui::style::Style::default().fg(TEXT))
            };

            // -- Match indicator --
            let match_line = if !self.new_password.is_empty() && !self.confirm_password.is_empty() {
                let passwords_match = self
                    .new_password
                    .expose(|a| self.confirm_password.expose(|b| a == b));
                if passwords_match {
                    Some(
                        Paragraph::new(format!(
                            "{} {}",
                            theme::ICON_SUCCESS,
                            t!("tui.entry.password_match")
                        ))
                        .style(Styles::success_text()),
                    )
                } else {
                    None
                }
            } else {
                None
            };

            // -- Error message --
            let error_line = self.error_message.as_ref().map(|msg| {
                Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg))
                    .style(Styles::error_text())
                    .wrap(Wrap { trim: true })
            });

            // -- Hint --
            let hint = Paragraph::new(t!("tui.entry.input_hint"))
                .style(ratatui::style::Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);

            // -- Layout rows --
            let rows = Layout::vertical([
                Constraint::Length(1), // title
                Constraint::Length(1), // gap
                Constraint::Length(3), // new password input with borders
                Constraint::Length(1), // strength bar
                Constraint::Length(1), // gap
                Constraint::Length(3), // confirm password input with borders
                Constraint::Length(1), // match indicator or gap
                Constraint::Length(1), // error or gap
                Constraint::Length(1), // gap
                Constraint::Length(1), // hint
            ])
            .split(content_area);

            frame.render_widget(title, rows[0]);

            // New password field
            frame.render_widget(new_input_block, rows[2]);
            let new_inner = Layout::vertical([Constraint::Length(1)]).split(rows[2])[0];
            let new_padded =
                Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(new_inner);
            frame.render_widget(new_input_text, new_padded[1]);

            // Strength bar
            frame.render_widget(strength_line, rows[3]);

            // Confirm password field
            frame.render_widget(confirm_input_block, rows[5]);
            let confirm_inner = Layout::vertical([Constraint::Length(1)]).split(rows[5])[0];
            let confirm_padded = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)])
                .split(confirm_inner);
            frame.render_widget(confirm_input_text, confirm_padded[1]);

            // Match indicator
            if let Some(ref ml) = match_line {
                frame.render_widget(ml.clone(), rows[6]);
            }

            // Error message
            if let Some(ref el) = error_line {
                frame.render_widget(el.clone(), rows[7]);
            }

            // Hint
            frame.render_widget(hint, rows[9]);
        }
    }

    fn on_mount(&mut self, _ctx: &mut ScreenContext) {
        self.step = 1;
        self.current_password.clear();
        self.new_password.clear();
        self.confirm_password.clear();
        self.focused = PasswordField::New;
        self.error_message = None;
        self.password_strength = None;
        self.pending_verification = false;
    }

    fn on_unmount(&mut self) {
        self.current_password.clear();
        self.new_password.clear();
        self.confirm_password.clear();
    }
}

// ── Private helpers ──────────────────────────────────────────────────────────

impl ChangeMasterPasswordScreen {
    /// Render step 1: verify current password.
    fn view_step1(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        use ratatui::layout::{Alignment, Constraint, Layout};
        use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

        // Vertical centering
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(10),
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

        // Title
        let title = Paragraph::new(t!("tui.entry.verify_current_title"))
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        // Subtitle
        let subtitle = Paragraph::new(t!("tui.entry.verify_current_hint"))
            .style(ratatui::style::Style::default().fg(theme::TEXT_SECONDARY))
            .alignment(Alignment::Center);

        // Password field
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::focused_border())
            .title(" Current Password ");

        let display = if self.current_password.is_empty() {
            String::new()
        } else {
            self.current_password.expose(Self::display_password)
        };
        let placeholder = if self.current_password.is_empty() {
            t!("tui.entry.verify_current_placeholder")
        } else {
            std::borrow::Cow::Borrowed("")
        };

        let input_text = if display.is_empty() {
            Paragraph::new(placeholder).style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(display).style(ratatui::style::Style::default().fg(TEXT))
        };

        // Error message
        let error_line = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg))
                .style(Styles::error_text())
                .wrap(Wrap { trim: true })
        });

        // Hint
        let hint = Paragraph::new("Enter: verify | Esc: back")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);

        // Layout rows
        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // subtitle
            Constraint::Length(1), // gap
            Constraint::Length(3), // password input with borders
            Constraint::Length(1), // error or gap
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(subtitle, rows[1]);

        // Password input
        frame.render_widget(input_block, rows[3]);
        let inner = Layout::vertical([Constraint::Length(1)]).split(rows[3])[0];
        let padded = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(inner);
        frame.render_widget(input_text, padded[1]);

        // Error
        if let Some(ref el) = error_line {
            frame.render_widget(el.clone(), rows[4]);
        }

        // Hint
        frame.render_widget(hint, rows[6]);
    }
}

// ── Key handling ─────────────────────────────────────────────────────────────

impl ChangeMasterPasswordScreen {
    fn handle_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Esc => ScreenResult::PopScreen,

            KeyCode::Enter if self.step == 1 => {
                if self.pending_verification {
                    return ScreenResult::Continue;
                }
                if self.current_password.is_empty() {
                    self.error_message = Some(t!("tui.entry.password_empty").to_string());
                    return ScreenResult::Continue;
                }
                self.error_message = None;
                let password = self.current_password.take_secure();
                let cmd = Command::VerifyMasterPassword { password };
                self.pending_verification = ctx.command_tx.try_send(cmd).is_ok();
                ScreenResult::Continue
            }

            KeyCode::Enter if self.step == 2 => {
                if self.new_password.len() < 8 {
                    self.error_message = Some(t!("tui.entry.password_too_short").to_string());
                    return ScreenResult::Continue;
                }
                let passwords_match = self
                    .new_password
                    .expose(|a| self.confirm_password.expose(|b| a == b));
                if !passwords_match {
                    self.error_message = Some(t!("tui.entry.password_mismatch").to_string());
                    return ScreenResult::Continue;
                }
                self.error_message = None;
                let new_pw = self.new_password.take_secure();
                self.confirm_password.clear();
                let cmd = Command::ChangeMasterPassword {
                    current_password: None,
                    new_password: new_pw,
                };
                let _ = ctx.command_tx.try_send(cmd);
                ScreenResult::Continue
            }

            KeyCode::Tab if self.step == 2 => {
                self.focused = match self.focused {
                    PasswordField::New => PasswordField::Confirm,
                    PasswordField::Confirm => PasswordField::New,
                };
                ScreenResult::Continue
            }

            KeyCode::Backspace => {
                if self.step == 1 {
                    self.current_password.pop_char();
                } else {
                    match self.focused {
                        PasswordField::New => {
                            self.new_password.pop_char();
                            self.update_strength();
                        }
                        PasswordField::Confirm => {
                            self.confirm_password.pop_char();
                        }
                    }
                }
                self.error_message = None;
                ScreenResult::Continue
            }

            KeyCode::Char(c) => {
                if self.step == 1 {
                    self.current_password.push_char(c);
                } else {
                    match self.focused {
                        PasswordField::New => {
                            self.new_password.push_char(c);
                            self.update_strength();
                        }
                        PasswordField::Confirm => {
                            self.confirm_password.push_char(c);
                        }
                    }
                }
                self.error_message = None;
                ScreenResult::Continue
            }

            _ => ScreenResult::Continue,
        }
    }

    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::MasterPasswordVerified => {
                // Verification succeeded — advance to new-password step.
                self.step = 2;
                self.pending_verification = false;
                ScreenResult::Continue
            }
            CommandResult::MasterPasswordChanged => ScreenResult::PopScreen,
            CommandResult::Error { fallback, .. } => {
                self.error_message = Some(fallback);
                if self.pending_verification {
                    // Verification failed — stay on step 1, user can retry.
                    self.pending_verification = false;
                } else {
                    // Change failed — reset to step 1 for safety.
                    self.step = 1;
                    self.current_password.clear();
                    self.new_password.clear();
                    self.confirm_password.clear();
                }
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::traits::screen::Screen as ScreenTrait;

    fn sensitive(s: &str) -> SensitiveInput {
        let mut input = SensitiveInput::new();
        for c in s.chars() {
            input.push_char(c);
        }
        input
    }

    #[test]
    fn new_screen_starts_at_step1() {
        let screen = ChangeMasterPasswordScreen::new();
        assert_eq!(screen.step, 1);
        assert!(screen.current_password.is_empty());
        assert!(screen.new_password.is_empty());
        assert!(screen.confirm_password.is_empty());
        assert!(screen.error_message.is_none());
        assert!(screen.password_strength.is_none());
    }

    #[test]
    fn on_mount_resets_state() {
        let mut screen = ChangeMasterPasswordScreen::new();
        screen.step = 2;
        screen.current_password = sensitive("old_pw");
        screen.new_password = sensitive("new_pw");
        screen.confirm_password = sensitive("new_pw");
        screen.error_message = Some("error".to_string());
        screen.pending_verification = true;

        // Need a dummy context for on_mount
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let config = crate::config::AppConfig::default();
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &config,
        };
        ScreenTrait::on_mount(&mut screen, &mut ctx);

        assert_eq!(screen.step, 1);
        assert!(screen.current_password.is_empty());
        assert!(screen.new_password.is_empty());
        assert!(screen.confirm_password.is_empty());
        assert!(screen.error_message.is_none());
        assert!(screen.password_strength.is_none());
        assert!(!screen.pending_verification);
    }

    #[test]
    fn on_unmount_clears_sensitive_data() {
        let mut screen = ChangeMasterPasswordScreen::new();
        screen.current_password = sensitive("sensitive1");
        screen.new_password = sensitive("sensitive2");
        screen.confirm_password = sensitive("sensitive3");
        ScreenTrait::on_unmount(&mut screen);
        assert!(screen.current_password.is_empty());
        assert!(screen.new_password.is_empty());
        assert!(screen.confirm_password.is_empty());
    }

    #[test]
    fn strength_updates_on_new_password() {
        let mut screen = ChangeMasterPasswordScreen::new();
        assert!(screen.password_strength.is_none());

        screen.new_password = sensitive("a");
        screen.update_strength();
        assert_eq!(
            screen.password_strength.as_ref().unwrap().level,
            StrengthLevel::VeryWeak
        );

        screen.new_password = sensitive("abcd1234ABCD!@ab");
        screen.update_strength();
        assert_eq!(
            screen.password_strength.as_ref().unwrap().level,
            StrengthLevel::Strong
        );

        screen.new_password.clear();
        screen.update_strength();
        assert!(screen.password_strength.is_none());
    }

    #[test]
    fn display_password_masks() {
        let displayed = ChangeMasterPasswordScreen::display_password("hello");
        assert_eq!(displayed, "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
        assert!(!displayed.contains('h'));
    }

    #[test]
    fn strength_color_mapping() {
        assert_eq!(
            ChangeMasterPasswordScreen::strength_color(&StrengthLevel::VeryWeak),
            ERROR
        );
        assert_eq!(
            ChangeMasterPasswordScreen::strength_color(&StrengthLevel::Weak),
            ERROR
        );
        assert_eq!(
            ChangeMasterPasswordScreen::strength_color(&StrengthLevel::Fair),
            WARNING
        );
        assert_eq!(
            ChangeMasterPasswordScreen::strength_color(&StrengthLevel::Strong),
            PRIMARY
        );
        assert_eq!(
            ChangeMasterPasswordScreen::strength_color(&StrengthLevel::VeryStrong),
            SUCCESS
        );
    }

    #[test]
    fn tab_toggles_focus_in_step2() {
        let mut screen = ChangeMasterPasswordScreen::new();
        screen.step = 2;
        assert_eq!(screen.focused, PasswordField::New);

        screen.focused = PasswordField::Confirm;
        assert_eq!(screen.focused, PasswordField::Confirm);

        screen.focused = PasswordField::New;
        assert_eq!(screen.focused, PasswordField::New);
    }
}
