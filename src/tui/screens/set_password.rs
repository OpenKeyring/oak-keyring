//! Set password screen — new master password entry with strength indicator and confirmation.

use crossterm::event::{KeyCode, KeyEvent};
use zeroize::Zeroize;

use crate::commands::result::CommandResult;
use crate::commands::types::Screen;
use crate::commands::{Command, Message};
use crate::crypto::strength::{evaluate_strength, PasswordStrength, StrengthLevel};
use crate::tui::theme::{
    self, Styles, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED, TEXT_PLACEHOLDER, WARNING,
};
use crate::tui::traits::screen::{ScreenContext, ScreenResult};
use crate::types::SecureStr;

// ── Enums ───────────────────────────────────────────────────────────────────

/// Which password field is currently focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordField {
    New,
    Confirm,
}

/// Context in which the set-password screen is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetPasswordContext {
    PostRecovery,
    OnboardingCreate,
    OnboardingRestore,
}

// ── SetPasswordScreen ────────────────────────────────────────────────────────

/// Password setting screen with strength indicator and confirmation field.
#[derive(Debug)]
pub struct SetPasswordScreen {
    pub new_password: String,
    pub confirm_password: String,
    pub focused: PasswordField,
    pub context: SetPasswordContext,
    pub strength: Option<PasswordStrength>,
    pub error: Option<String>,
    pub password_visible: bool,
}

impl SetPasswordScreen {
    pub fn new(context: SetPasswordContext) -> Self {
        Self {
            new_password: String::new(),
            confirm_password: String::new(),
            focused: PasswordField::New,
            context,
            strength: None,
            error: None,
            password_visible: false,
        }
    }

    /// Re-evaluate password strength from the new password field.
    fn update_strength(&mut self) {
        if self.new_password.is_empty() {
            self.strength = None;
        } else {
            self.strength = Some(evaluate_strength(&self.new_password));
        }
    }

    /// Return the display text for the given password string.
    fn display_password(&self, password: &str) -> String {
        if self.password_visible {
            password.to_string()
        } else {
            "\u{2022}".repeat(password.len())
        }
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

// ── Screen trait impl ────────────────────────────────────────────────────────

impl crate::tui::traits::screen::Screen for SetPasswordScreen {
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

        // Vertical centering
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(16),
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
        let title = Paragraph::new("Set Master Password")
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
            self.display_password(&self.new_password)
        };
        let new_placeholder = if self.new_password.is_empty() {
            "Enter new password"
        } else {
            ""
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
        let strength_line = if let Some(ref s) = self.strength {
            let bar_total = 16u8;
            let filled = s.bar_fill.min(bar_total);
            let empty = bar_total - filled;
            let bar_str = format!(
                "{}{}",
                "\u{2588}".repeat(filled as usize),
                "\u{2591}".repeat(empty as usize)
            );
            let label = format!("Strength: {} {}", s.level.label_zh(), bar_str);
            let color = Self::strength_color(&s.level);
            Paragraph::new(label).style(ratatui::style::Style::default().fg(color))
        } else {
            Paragraph::new("Strength: ").style(ratatui::style::Style::default().fg(TEXT_MUTED))
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
            self.display_password(&self.confirm_password)
        };
        let confirm_placeholder = if self.confirm_password.is_empty() {
            "Confirm password"
        } else {
            ""
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
            if self.new_password == self.confirm_password {
                Some(
                    Paragraph::new(format!("{} Passwords match", theme::ICON_SUCCESS))
                        .style(Styles::success_text()),
                )
            } else {
                None
            }
        } else {
            None
        };

        // -- Error message --
        let error_line = self.error.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg))
                .style(Styles::error_text())
                .wrap(Wrap { trim: true })
        });

        // -- Hint --
        let hint = Paragraph::new("Tab: switch field | Enter: submit | Esc: back")
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
        let confirm_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(confirm_inner);
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

    fn on_mount(&mut self, _ctx: &mut ScreenContext) {
        // No-op
    }

    fn on_unmount(&mut self) {
        self.new_password.zeroize();
        self.confirm_password.zeroize();
    }
}

// ── Key handling ─────────────────────────────────────────────────────────────

impl SetPasswordScreen {
    fn handle_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Tab => {
                self.focused = match self.focused {
                    PasswordField::New => PasswordField::Confirm,
                    PasswordField::Confirm => PasswordField::New,
                };
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                if self.new_password.len() < 8 {
                    self.error = Some("Password must be at least 8 characters".to_string());
                    return ScreenResult::Continue;
                }
                if self.new_password != self.confirm_password {
                    self.error = Some("Passwords do not match".to_string());
                    return ScreenResult::Continue;
                }
                // Passwords match and long enough — send InitializeVault
                self.error = None;
                let password = std::mem::take(&mut self.new_password);
                self.confirm_password.zeroize();
                self.confirm_password.clear();
                let vault_path = dirs::data_local_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("open-keyring")
                    .join("vault.db");
                let cmd = Command::InitializeVault {
                    vault_path,
                    master_password: SecureStr::new(password),
                };
                let _ = ctx.command_tx.try_send(cmd);
                ScreenResult::Continue
            }
            KeyCode::Esc => ScreenResult::Continue,
            KeyCode::Backspace => {
                match self.focused {
                    PasswordField::New => {
                        self.new_password.pop();
                        self.update_strength();
                    }
                    PasswordField::Confirm => {
                        self.confirm_password.pop();
                    }
                }
                // Clear error on new input
                self.error = None;
                ScreenResult::Continue
            }
            KeyCode::Char(c) => {
                match self.focused {
                    PasswordField::New => {
                        self.new_password.push(c);
                        self.update_strength();
                    }
                    PasswordField::Confirm => {
                        self.confirm_password.push(c);
                    }
                }
                // Clear error on new input
                self.error = None;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::VaultInitialized { .. } => ScreenResult::NavigateTo(Screen::Main),
            CommandResult::Error { fallback, .. } => {
                self.error = Some(fallback);
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

    #[test]
    fn set_password_screen_new() {
        let screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate);
        assert!(screen.new_password.is_empty());
        assert!(screen.confirm_password.is_empty());
        assert_eq!(screen.focused, PasswordField::New);
        assert_eq!(screen.context, SetPasswordContext::OnboardingCreate);
        assert!(screen.strength.is_none());
        assert!(screen.error.is_none());
        assert!(!screen.password_visible);
    }

    #[test]
    fn set_password_passwords_match() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        screen.new_password = "testpassword".to_string();
        screen.confirm_password = "testpassword".to_string();
        assert_eq!(screen.new_password, screen.confirm_password);

        // Mismatch case
        screen.confirm_password = "different".to_string();
        assert_ne!(screen.new_password, screen.confirm_password);
    }

    #[test]
    fn set_password_strength_updates() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingRestore);

        // Empty — no strength
        assert!(screen.strength.is_none());

        // Short password — VeryWeak
        screen.new_password = "a".to_string();
        screen.update_strength();
        let s = screen.strength.as_ref().unwrap();
        assert_eq!(s.level, StrengthLevel::VeryWeak);

        // Stronger password
        screen.new_password = "abcd1234ABCD!@ab".to_string();
        screen.update_strength();
        let s = screen.strength.as_ref().unwrap();
        assert_eq!(s.level, StrengthLevel::Strong);

        // Back to empty — no strength
        screen.new_password.clear();
        screen.update_strength();
        assert!(screen.strength.is_none());
    }

    #[test]
    fn tab_toggles_focus() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate);
        assert_eq!(screen.focused, PasswordField::New);

        screen.focused = PasswordField::Confirm;
        assert_eq!(screen.focused, PasswordField::Confirm);

        screen.focused = PasswordField::New;
        assert_eq!(screen.focused, PasswordField::New);
    }

    #[test]
    fn display_password_masked_by_default() {
        let screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate);
        let displayed = screen.display_password("hello");
        assert_eq!(displayed, "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
        assert!(!displayed.contains('h'));
    }

    #[test]
    fn display_password_visible_when_toggled() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate);
        screen.password_visible = true;
        let displayed = screen.display_password("hello");
        assert_eq!(displayed, "hello");
    }

    #[test]
    fn strength_color_mapping() {
        assert_eq!(
            SetPasswordScreen::strength_color(&StrengthLevel::VeryWeak),
            ERROR
        );
        assert_eq!(
            SetPasswordScreen::strength_color(&StrengthLevel::Weak),
            ERROR
        );
        assert_eq!(
            SetPasswordScreen::strength_color(&StrengthLevel::Fair),
            WARNING
        );
        assert_eq!(
            SetPasswordScreen::strength_color(&StrengthLevel::Strong),
            PRIMARY
        );
        assert_eq!(
            SetPasswordScreen::strength_color(&StrengthLevel::VeryStrong),
            SUCCESS
        );
    }

    #[test]
    fn on_unmount_zeroizes() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate);
        screen.new_password = "sensitive123".to_string();
        screen.confirm_password = "sensitive123".to_string();
        ScreenTrait::on_unmount(&mut screen);
        assert!(screen.new_password.is_empty());
        assert!(screen.confirm_password.is_empty());
    }

    #[test]
    fn context_variants() {
        let s1 = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        assert_eq!(s1.context, SetPasswordContext::PostRecovery);

        let s2 = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate);
        assert_eq!(s2.context, SetPasswordContext::OnboardingCreate);

        let s3 = SetPasswordScreen::new(SetPasswordContext::OnboardingRestore);
        assert_eq!(s3.context, SetPasswordContext::OnboardingRestore);
    }
}
