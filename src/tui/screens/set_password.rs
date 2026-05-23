//! Set password screen — new master password entry with strength indicator and confirmation.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use crate::commands::result::CommandResult;
use crate::commands::types::Screen;
use crate::commands::{Command, Message};
use crate::crypto::strength::{evaluate_strength, PasswordStrength, StrengthLevel};
use crate::t;
use crate::tui::screens::onboarding::views_setup::{header_rows, render_header};
use crate::tui::terminal::WidthTier;
use crate::tui::theme::{
    self, Styles, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED, TEXT_PLACEHOLDER, WARNING,
};
use crate::tui::traits::screen::{ScreenContext, ScreenResult};
use crate::types::sensitive::SensitiveInput;
use crate::types::RecoveryWords;

fn contains(area: ratatui::layout::Rect, col: u16, row: u16) -> bool {
    area.left() <= col && col < area.right() && area.top() <= row && row < area.bottom()
}

// ── Enums ───────────────────────────────────────────────────────────────────

/// Which password field is currently focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PasswordField {
    #[default]
    New,
    Confirm,
}

/// Context in which the set-password screen is shown.
#[derive(Debug)]
pub enum SetPasswordContext {
    PostRecovery,
    OnboardingCreate {
        recovery_words: RecoveryWords,
    },
    OnboardingRestore,
    /// Rebuild wrapped_secret_key.json from recovered key + new master password.
    RestoreExistingVault {
        recovery_words: RecoveryWords,
        next: RestoreNext,
    },
}

/// What happens after keyfile is rebuilt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreNext {
    /// Validate the existing database (no-key + has-db flow).
    ValidateExistingDatabase,
    /// Proceed to database recovery screen (no-key + no-db flow).
    RestoreDatabase,
}

impl Drop for SetPasswordContext {
    fn drop(&mut self) {
        // RecoveryWords has its own Drop impl that zeroizes contents.
        // No manual cleanup needed here.
    }
}

// ── SetPasswordScreen ────────────────────────────────────────────────────────

/// Password setting screen with strength indicator and confirmation field.
#[derive(Debug)]
pub struct SetPasswordScreen {
    pub new_password: SensitiveInput,
    pub confirm_password: SensitiveInput,
    pub focused: PasswordField,
    pub context: SetPasswordContext,
    pub strength: Option<PasswordStrength>,
    pub error: Option<String>,
    pub password_visible: bool,
    pub new_input_area: std::cell::Cell<ratatui::layout::Rect>,
    pub confirm_input_area: std::cell::Cell<ratatui::layout::Rect>,
}

impl Default for SetPasswordScreen {
    fn default() -> Self {
        Self::new(SetPasswordContext::PostRecovery)
    }
}

impl SetPasswordScreen {
    pub fn new(context: SetPasswordContext) -> Self {
        Self {
            new_password: SensitiveInput::new(),
            confirm_password: SensitiveInput::new(),
            focused: PasswordField::New,
            context,
            strength: None,
            error: None,
            password_visible: false,
            new_input_area: std::cell::Cell::new(ratatui::layout::Rect::default()),
            confirm_input_area: std::cell::Cell::new(ratatui::layout::Rect::default()),
        }
    }

    /// Re-evaluate password strength from the new password field.
    fn update_strength(&mut self) {
        if self.new_password.is_empty() {
            self.strength = None;
        } else {
            self.new_password.expose(|s| {
                self.strength = Some(evaluate_strength(s));
            });
        }
    }

    /// Return the display text for the given password string.
    fn display_password(&self, password: &str) -> String {
        if self.password_visible {
            password.to_string()
        } else {
            theme::ICON_PASSWORD_MASK.repeat(password.chars().count())
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
            Message::MouseEvent(event) => self.handle_mouse(event),
            Message::CommandCompleted(result) => self.handle_command_result(result),
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        use ratatui::layout::{Alignment, Constraint, Layout};
        use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

        let wide = WidthTier::from_width(area.width) != WidthTier::TooSmall;
        let header_height = Self::header_height(area, wide);
        let content_height = 16 + header_height;

        // Vertical centering
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(content_height),
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
        let (header_area, form_area) = if header_height > 0 {
            let sections =
                Layout::vertical([Constraint::Length(header_height), Constraint::Length(16)])
                    .split(content_area);
            (Some(sections[0]), sections[1])
        } else {
            (None, content_area)
        };

        if let Some(header_area) = header_area {
            render_header(frame, header_area, wide);
        }

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
            self.new_password.expose(|s| self.display_password(s))
        };
        let new_placeholder = if self.new_password.is_empty() {
            t!("tui.entry.new_password_placeholder")
        } else {
            std::borrow::Cow::Borrowed("")
        };

        let new_input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(new_border_style)
            .title(t!("tui.entry.new_password_title").to_string());

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
            self.confirm_password.expose(|s| self.display_password(s))
        };
        let confirm_placeholder = if self.confirm_password.is_empty() {
            t!("tui.entry.confirm_password")
        } else {
            std::borrow::Cow::Borrowed("")
        };

        let confirm_input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(confirm_border_style)
            .title(t!("tui.entry.confirm_new_password_title").to_string());

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
        let error_line = self.error.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg))
                .style(Styles::error_text())
                .wrap(Wrap { trim: true })
        });

        // -- Hint --
        let hint = Paragraph::new(t!("tui.entry.set_password_input_hint"))
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
        .split(form_area);

        frame.render_widget(title, rows[0]);

        // New password field
        let new_inner = new_input_block.inner(rows[2]);
        self.new_input_area.set(rows[2]);
        frame.render_widget(new_input_block, rows[2]);
        frame.render_widget(new_input_text, new_inner);

        // Strength bar
        frame.render_widget(strength_line, rows[3]);

        // Confirm password field
        let confirm_inner = confirm_input_block.inner(rows[5]);
        self.confirm_input_area.set(rows[5]);
        frame.render_widget(confirm_input_block, rows[5]);
        frame.render_widget(confirm_input_text, confirm_inner);

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
        self.new_password.clear();
        self.confirm_password.clear();
        self.error = None;
        // RecoveryWords in context zeroize on drop/replacement.
        self.context = SetPasswordContext::PostRecovery;
    }
}

// ── Key handling ─────────────────────────────────────────────────────────────

impl SetPasswordScreen {
    fn header_height(area: ratatui::layout::Rect, wide: bool) -> u16 {
        let header_height = header_rows(wide);
        if area.height >= 16 + header_height {
            header_height
        } else {
            0
        }
    }

    fn cycle_focus_forward(&mut self) {
        self.focused = match self.focused {
            PasswordField::New => PasswordField::Confirm,
            PasswordField::Confirm => PasswordField::New,
        };
    }

    fn cycle_focus_backward(&mut self) {
        self.cycle_focus_forward();
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Tab | KeyCode::Down => {
                self.cycle_focus_forward();
                ScreenResult::Continue
            }
            KeyCode::Up => {
                self.cycle_focus_backward();
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                if self.new_password.len() < 8 {
                    self.error = Some(t!("tui.entry.password_too_short").to_string());
                    return ScreenResult::Continue;
                }
                let passwords_match = self
                    .new_password
                    .expose(|a| self.confirm_password.expose(|b| a == b));
                if !passwords_match {
                    self.error = Some(t!("tui.entry.password_mismatch").to_string());
                    return ScreenResult::Continue;
                }
                // Passwords match and long enough
                self.error = None;
                let password = self.new_password.take_secure();
                self.confirm_password.clear();
                let cmd = match &self.context {
                    SetPasswordContext::RestoreExistingVault { recovery_words, .. } => {
                        match recovery_words.duplicate_for_command() {
                            Ok(recovery_words) => Command::RebuildKeyFileFromRecovery {
                                master_password: password,
                                recovery_words,
                            },
                            Err(_) => {
                                self.error =
                                    Some(t!("tui.entry.key_recovery_empty_error").to_string());
                                return ScreenResult::Continue;
                            }
                        }
                    }
                    SetPasswordContext::OnboardingCreate { recovery_words } => match recovery_words
                        .duplicate_for_command()
                    {
                        Ok(recovery_words) => Command::InitializeVault {
                            master_password: password,
                            recovery_words: Some(recovery_words),
                        },
                        Err(_) => {
                            self.error = Some(t!("tui.entry.key_recovery_empty_error").to_string());
                            return ScreenResult::Continue;
                        }
                    },
                    _ => Command::InitializeVault {
                        master_password: password,
                        recovery_words: None,
                    },
                };
                if ctx.command_tx.try_send(cmd).is_err() {
                    self.error = Some(t!("tui.error.command_dispatch_failed").to_string());
                }
                ScreenResult::Continue
            }
            KeyCode::Esc if self.can_go_back() => ScreenResult::PopScreen,
            KeyCode::Esc => {
                self.error = Some(t!("tui.entry.restart_onboarding_required").to_string());
                ScreenResult::Continue
            }
            KeyCode::Backspace => {
                match self.focused {
                    PasswordField::New => {
                        self.new_password.pop_char();
                        self.update_strength();
                    }
                    PasswordField::Confirm => {
                        self.confirm_password.pop_char();
                    }
                }
                // Clear error on new input
                self.error = None;
                ScreenResult::Continue
            }
            KeyCode::Char(c) => {
                match self.focused {
                    PasswordField::New => {
                        self.new_password.push_char(c);
                        self.update_strength();
                    }
                    PasswordField::Confirm => {
                        self.confirm_password.push_char(c);
                    }
                }
                // Clear error on new input
                self.error = None;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn can_go_back(&self) -> bool {
        matches!(self.context, SetPasswordContext::PostRecovery)
    }

    fn handle_mouse(&mut self, event: MouseEvent) -> ScreenResult {
        if !matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            return ScreenResult::Continue;
        }

        if contains(self.new_input_area.get(), event.column, event.row) {
            self.focused = PasswordField::New;
        } else if contains(self.confirm_input_area.get(), event.column, event.row) {
            self.focused = PasswordField::Confirm;
        }
        ScreenResult::Continue
    }

    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::VaultInitialized => ScreenResult::NavigateTo(Screen::Main),
            CommandResult::KeyFileRebuilt => match &self.context {
                SetPasswordContext::RestoreExistingVault {
                    next: RestoreNext::ValidateExistingDatabase,
                    ..
                } => ScreenResult::Command(Box::new(Command::ValidateRestoredDatabase)),
                SetPasswordContext::RestoreExistingVault {
                    next: RestoreNext::RestoreDatabase,
                    ..
                } => ScreenResult::NavigateTo(Screen::DatabaseRecovery),
                _ => ScreenResult::NavigateTo(Screen::Main),
            },
            CommandResult::Error { fallback, .. } => {
                self.error = Some(fallback);
                ScreenResult::Continue
            }
            CommandResult::DatabaseRestored { .. } => ScreenResult::NavigateTo(Screen::Main),
            CommandResult::DatabaseValidationFailed { reason } => {
                self.error = Some(reason);
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
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    fn sensitive(s: &str) -> SensitiveInput {
        let mut input = SensitiveInput::new();
        for c in s.chars() {
            input.push_char(c);
        }
        input
    }

    fn recovery_words() -> RecoveryWords {
        RecoveryWords::new((0..24).map(|i| format!("word{i}")).collect()).unwrap()
    }

    fn render_set_password(screen: &SetPasswordScreen, width: u16, height: u16) {
        let _ = render_set_password_buffer(screen, width, height);
    }

    fn render_set_password_buffer(screen: &SetPasswordScreen, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                screen.view(frame, frame.area());
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn click(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: crossterm::event::KeyModifiers::NONE,
        }
    }

    #[test]
    fn set_password_screen_new() {
        let screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate {
            recovery_words: recovery_words(),
        });
        assert!(screen.new_password.is_empty());
        assert!(screen.confirm_password.is_empty());
        assert_eq!(screen.focused, PasswordField::New);
        assert!(matches!(
            screen.context,
            SetPasswordContext::OnboardingCreate { .. }
        ));
        assert!(screen.strength.is_none());
        assert!(screen.error.is_none());
        assert!(!screen.password_visible);
    }

    #[test]
    fn set_password_passwords_match() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        screen.new_password = sensitive("testpassword");
        screen.confirm_password = sensitive("testpassword");
        let passwords_match = screen
            .new_password
            .expose(|a| screen.confirm_password.expose(|b| a == b));
        assert!(passwords_match);

        // Mismatch case
        screen.confirm_password = sensitive("different");
        let passwords_match = screen
            .new_password
            .expose(|a| screen.confirm_password.expose(|b| a == b));
        assert!(!passwords_match);
    }

    #[test]
    fn set_password_strength_updates() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingRestore);

        // Empty — no strength
        assert!(screen.strength.is_none());

        // Short password — VeryWeak
        screen.new_password = sensitive("a");
        screen.update_strength();
        let s = screen.strength.as_ref().unwrap();
        assert_eq!(s.level, StrengthLevel::VeryWeak);

        // Stronger password
        screen.new_password = sensitive("abcd1234ABCD!@ab");
        screen.update_strength();
        let s = screen.strength.as_ref().unwrap();
        assert_eq!(s.level, StrengthLevel::Strong);

        // Back to empty — no strength
        screen.new_password.clear();
        screen.update_strength();
        assert!(screen.strength.is_none());
    }

    #[test]
    fn onboarding_set_password_context_renders_logo_on_tall_terminal() {
        let screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate {
            recovery_words: recovery_words(),
        });

        let buffer = render_set_password_buffer(&screen, 80, 24);

        assert!(format!("{buffer:?}").contains("░█▀█"));
    }

    #[test]
    fn post_recovery_set_password_context_renders_logo_on_tall_terminal() {
        let screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);

        let buffer = render_set_password_buffer(&screen, 80, 24);

        assert!(format!("{buffer:?}").contains("░█▀█"));
    }

    #[test]
    fn tab_toggles_focus() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        assert_eq!(screen.focused, PasswordField::New);

        screen.focused = PasswordField::Confirm;
        assert_eq!(screen.focused, PasswordField::Confirm);

        screen.focused = PasswordField::New;
        assert_eq!(screen.focused, PasswordField::New);
    }

    #[test]
    fn arrows_cycle_focus_like_tab() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        let mut ctx = dummy_ctx();

        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Down,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert_eq!(screen.focused, PasswordField::Confirm);

        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Down,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert_eq!(screen.focused, PasswordField::New);

        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Up,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert_eq!(screen.focused, PasswordField::Confirm);
    }

    #[test]
    fn mouse_click_selects_password_field() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        render_set_password(&screen, 60, 16);
        let confirm_area = screen.confirm_input_area.get();
        let mut ctx = dummy_ctx();

        screen.update(
            Message::MouseEvent(click(confirm_area.x + 1, confirm_area.y + 1)),
            &mut ctx,
        );

        assert_eq!(screen.focused, PasswordField::Confirm);
    }

    #[test]
    fn esc_in_onboarding_context_prompts_restart_instead_of_popping() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate {
            recovery_words: recovery_words(),
        });
        let mut ctx = dummy_ctx();

        let result = screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Esc,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );

        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.error.as_deref().is_some_and(|msg| {
            msg.contains("restart onboarding") || msg.contains("重新开始")
        }));
    }

    #[test]
    fn display_password_masked_by_default() {
        let screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        let displayed = screen.display_password("hello");
        assert_eq!(displayed, "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
        assert!(!displayed.contains('h'));
    }

    #[test]
    fn display_password_visible_when_toggled() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
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
        let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate {
            recovery_words: recovery_words(),
        });
        screen.new_password = sensitive("sensitive123");
        screen.confirm_password = sensitive("sensitive123");
        ScreenTrait::on_unmount(&mut screen);
        assert!(screen.new_password.is_empty());
        assert!(screen.confirm_password.is_empty());
    }

    #[test]
    fn context_variants() {
        let s1 = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        assert!(matches!(s1.context, SetPasswordContext::PostRecovery));

        let s2 = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate {
            recovery_words: recovery_words(),
        });
        assert!(matches!(
            s2.context,
            SetPasswordContext::OnboardingCreate { .. }
        ));

        let s3 = SetPasswordScreen::new(SetPasswordContext::OnboardingRestore);
        assert!(matches!(s3.context, SetPasswordContext::OnboardingRestore));
    }

    // ── Key behavior tests (Issue #8 regression) ──────────────────────────────

    #[allow(static_mut_refs)]
    fn dummy_ctx() -> ScreenContext<'static> {
        static ONCE: std::sync::Once = std::sync::Once::new();
        static mut TX: Option<tokio::sync::mpsc::Sender<Command>> = None;

        ONCE.call_once(|| {
            let (tx, _rx) = tokio::sync::mpsc::channel(16);
            unsafe { TX = Some(tx) };
        });

        let tx = unsafe { TX.as_ref().unwrap() };
        static DUMMY_CONFIG: std::sync::OnceLock<crate::config::AppConfig> =
            std::sync::OnceLock::new();
        let config = DUMMY_CONFIG.get_or_init(crate::config::AppConfig::default);

        ScreenContext {
            command_tx: tx,
            config,
        }
    }

    #[test]
    fn esc_returns_pop_screen() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        let mut ctx = dummy_ctx();
        let result = screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Esc,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(
            matches!(result, ScreenResult::PopScreen),
            "Esc should return PopScreen for back-navigation"
        );
    }

    #[test]
    fn enter_rejects_short_password() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        let mut ctx = dummy_ctx();
        // Type a 5-character password
        for ch in "short".chars() {
            screen.update(
                Message::KeyEvent(KeyEvent::new(
                    KeyCode::Char(ch),
                    crossterm::event::KeyModifiers::NONE,
                )),
                &mut ctx,
            );
        }
        // Tab to confirm and type same short password
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Tab,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        for ch in "short".chars() {
            screen.update(
                Message::KeyEvent(KeyEvent::new(
                    KeyCode::Char(ch),
                    crossterm::event::KeyModifiers::NONE,
                )),
                &mut ctx,
            );
        }
        let result = screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(
            screen.error.as_deref(),
            Some(&*t!("tui.entry.password_too_short").to_string())
        );
    }

    #[test]
    fn enter_rejects_mismatched_passwords() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        let mut ctx = dummy_ctx();
        // Type password in new field
        for ch in "longpassword".chars() {
            screen.update(
                Message::KeyEvent(KeyEvent::new(
                    KeyCode::Char(ch),
                    crossterm::event::KeyModifiers::NONE,
                )),
                &mut ctx,
            );
        }
        // Tab and type different password in confirm
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Tab,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        for ch in "differentpass".chars() {
            screen.update(
                Message::KeyEvent(KeyEvent::new(
                    KeyCode::Char(ch),
                    crossterm::event::KeyModifiers::NONE,
                )),
                &mut ctx,
            );
        }
        let result = screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(
            screen.error.as_deref(),
            Some(&*t!("tui.entry.password_mismatch").to_string())
        );
    }

    #[test]
    fn vault_initialized_navigates_to_main() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        let result = screen.handle_command_result(CommandResult::VaultInitialized);
        assert!(matches!(result, ScreenResult::NavigateTo(Screen::Main)));
    }

    #[test]
    fn command_error_sets_error_message() {
        let mut screen = SetPasswordScreen::new(SetPasswordContext::PostRecovery);
        let result = screen.handle_command_result(CommandResult::Error {
            code: crate::errors::ErrorCode::CryptoEncryptionFailed,
            context: crate::errors::ErrorContext::new(),
            message_key: "vault.init_failed",
            fallback: "Failed to initialize vault".to_string(),
        });
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.error.as_deref(), Some("Failed to initialize vault"));
    }

    #[test]
    fn default_impl_uses_post_recovery_context() {
        let screen = SetPasswordScreen::default();
        assert!(matches!(screen.context, SetPasswordContext::PostRecovery));
        assert!(screen.new_password.is_empty());
    }

    #[test]
    fn enter_passes_recovery_words_in_initialize_vault_command() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Command>(16);
        let config = crate::config::AppConfig::default();
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &config,
        };

        let test_words = (0..24).map(|i| format!("word{i}")).collect::<Vec<_>>();
        let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate {
            recovery_words: RecoveryWords::new(test_words).unwrap(),
        });

        // Type matching 8+ char passwords
        for ch in "longpassword".chars() {
            screen.update(
                Message::KeyEvent(KeyEvent::new(
                    KeyCode::Char(ch),
                    crossterm::event::KeyModifiers::NONE,
                )),
                &mut ctx,
            );
        }
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Tab,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        for ch in "longpassword".chars() {
            screen.update(
                Message::KeyEvent(KeyEvent::new(
                    KeyCode::Char(ch),
                    crossterm::event::KeyModifiers::NONE,
                )),
                &mut ctx,
            );
        }
        screen.update(
            Message::KeyEvent(KeyEvent::new(
                KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );

        // Verify the command carries recovery_words
        let cmd = rx.try_recv().expect("Command should be sent");
        match cmd {
            Command::InitializeVault { recovery_words, .. } => {
                let words =
                    recovery_words.expect("recovery_words should be Some for OnboardingCreate");
                assert_eq!(words.len(), 24, "Should carry 24 recovery words");
                let expected_words = (0..24).map(|i| format!("word{i}")).collect::<Vec<_>>();
                assert_eq!(
                    words.as_slice(),
                    expected_words.as_slice(),
                    "Should carry the exact words from context"
                );
            }
            _ => panic!("Expected InitializeVault command"),
        }
    }
}
