//! Database recovery screen — restore vault.db from cloud or .okb backup.
//!
//! Used when:
//! - Startup detects `has key + no db` (routed to DatabaseRecovery screen).
//! - Onboarding "Restore existing vault" after key recovery (step 3 of 3).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::commands::result::CommandResult;
use crate::commands::{Command, Message};
use crate::t;
use crate::tui::screens::onboarding::views_setup::{header_rows, render_header};
use crate::tui::terminal::WidthTier;
use crate::tui::theme::{self, BRAND, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED};
use crate::tui::traits::screen::{Screen as ScreenTrait, ScreenContext, ScreenResult};
use crate::types::sensitive::SensitiveInput;

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseRecoveryOrigin {
    /// Startup: key file exists but vault.db is missing.
    StartupKeyOnly,
    /// Onboarding: key recovery done, now restore database.
    OnboardingRestore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseRecoveryMode {
    SourceSelection,
    OkbPathInput,
    OkbPasswordInput,
    OkbMasterPasswordInput,
    CloudSyncing,
    CloudMasterPasswordInput,
    CloudNeedsOAuth,
    CloudFailed,
    CloudSucceeded,
    OkbSucceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseRecoveryFocus {
    Cloud,
    Okb,
}

// ── DatabaseRecoveryScreen ─────────────────────────────────────────────────

#[derive(Debug)]
pub struct DatabaseRecoveryScreen {
    pub origin: DatabaseRecoveryOrigin,
    pub mode: DatabaseRecoveryMode,
    pub focus: DatabaseRecoveryFocus,
    pub okb_path: String,
    pub okb_password: SensitiveInput,
    pub master_password: SensitiveInput,
    pub error: Option<String>,
    pub progress: Option<(usize, usize, String)>,
}

impl Default for DatabaseRecoveryScreen {
    fn default() -> Self {
        Self::new(DatabaseRecoveryOrigin::StartupKeyOnly)
    }
}

impl DatabaseRecoveryScreen {
    pub fn new(origin: DatabaseRecoveryOrigin) -> Self {
        Self {
            origin,
            mode: DatabaseRecoveryMode::SourceSelection,
            focus: DatabaseRecoveryFocus::Cloud,
            okb_path: String::new(),
            okb_password: SensitiveInput::new(),
            master_password: SensitiveInput::new(),
            error: None,
            progress: None,
        }
    }

    fn step_text(&self) -> std::borrow::Cow<'static, str> {
        match self.origin {
            DatabaseRecoveryOrigin::StartupKeyOnly => t!("tui.entry.db_recovery_step_1_1"),
            DatabaseRecoveryOrigin::OnboardingRestore => t!("tui.entry.db_recovery_step_3_3"),
        }
    }

    fn handle_source_selection(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Down | KeyCode::Tab => {
                self.focus = match self.focus {
                    DatabaseRecoveryFocus::Cloud => DatabaseRecoveryFocus::Okb,
                    DatabaseRecoveryFocus::Okb => DatabaseRecoveryFocus::Cloud,
                };
                ScreenResult::Continue
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.focus = match self.focus {
                    DatabaseRecoveryFocus::Cloud => DatabaseRecoveryFocus::Okb,
                    DatabaseRecoveryFocus::Okb => DatabaseRecoveryFocus::Cloud,
                };
                ScreenResult::Continue
            }
            KeyCode::Enter if self.focus == DatabaseRecoveryFocus::Cloud => {
                if matches!(self.origin, DatabaseRecoveryOrigin::StartupKeyOnly) {
                    self.mode = DatabaseRecoveryMode::CloudMasterPasswordInput;
                    self.error = None;
                } else {
                    self.mode = DatabaseRecoveryMode::CloudSyncing;
                    ctx.send_system_command(Command::RestoreDatabaseFromCloud {
                        master_password: None,
                    });
                }
                ScreenResult::Continue
            }
            KeyCode::Enter if self.focus == DatabaseRecoveryFocus::Okb => {
                self.mode = DatabaseRecoveryMode::OkbPathInput;
                self.error = None;
                ScreenResult::Continue
            }
            KeyCode::Esc => ScreenResult::PopScreen,
            _ => ScreenResult::Continue,
        }
    }

    fn handle_okb_input(&mut self, key: KeyEvent, _ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Enter => {
                let path = self.okb_path.trim();
                if path.is_empty() {
                    self.error = Some(t!("tui.entry.db_recovery_okb_empty_error").to_string());
                    ScreenResult::Continue
                } else if !path.ends_with(".okb") {
                    self.error = Some(t!("tui.entry.db_recovery_okb_ext_error").to_string());
                    ScreenResult::Continue
                } else {
                    self.error = None;
                    self.mode = DatabaseRecoveryMode::OkbPasswordInput;
                    ScreenResult::Continue
                }
            }
            KeyCode::Esc => {
                self.mode = DatabaseRecoveryMode::SourceSelection;
                self.error = None;
                ScreenResult::Continue
            }
            KeyCode::Char(c) => {
                self.okb_path.push(c);
                self.error = None;
                ScreenResult::Continue
            }
            KeyCode::Backspace => {
                self.okb_path.pop();
                self.error = None;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_okb_password(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Enter => {
                if self.okb_password.is_empty() {
                    self.error =
                        Some(t!("tui.entry.db_recovery_okb_password_empty_error").to_string());
                    ScreenResult::Continue
                } else {
                    self.error = None;
                    if matches!(self.origin, DatabaseRecoveryOrigin::StartupKeyOnly) {
                        self.mode = DatabaseRecoveryMode::OkbMasterPasswordInput;
                    } else {
                        let path = std::path::PathBuf::from(self.okb_path.trim());
                        let password = self.okb_password.take_secure();
                        ctx.send_system_command(Command::RestoreDatabaseFromOkb {
                            path,
                            password,
                            master_password: None,
                        });
                    }
                    ScreenResult::Continue
                }
            }
            KeyCode::Esc => {
                self.mode = DatabaseRecoveryMode::OkbPathInput;
                self.okb_password.clear();
                self.error = None;
                ScreenResult::Continue
            }
            KeyCode::Char(c) => {
                self.okb_password.push_char(c);
                self.error = None;
                ScreenResult::Continue
            }
            KeyCode::Backspace => {
                self.okb_password.pop_char();
                self.error = None;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_cloud_master_password(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match key.code {
            KeyCode::Enter => {
                if self.master_password.is_empty() {
                    self.error =
                        Some(t!("tui.entry.db_recovery_master_password_empty_error").to_string());
                } else {
                    self.error = None;
                    self.mode = DatabaseRecoveryMode::CloudSyncing;
                    let master_password = self.master_password.take_secure();
                    ctx.send_system_command(Command::RestoreDatabaseFromCloud {
                        master_password: Some(master_password),
                    });
                }
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.master_password.clear();
                self.error = None;
                self.mode = DatabaseRecoveryMode::SourceSelection;
                ScreenResult::Continue
            }
            KeyCode::Char(c) => {
                self.master_password.push_char(c);
                self.error = None;
                ScreenResult::Continue
            }
            KeyCode::Backspace => {
                self.master_password.pop_char();
                self.error = None;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_okb_master_password(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match key.code {
            KeyCode::Enter => {
                if self.master_password.is_empty() {
                    self.error =
                        Some(t!("tui.entry.db_recovery_master_password_empty_error").to_string());
                } else {
                    self.error = None;
                    let path = std::path::PathBuf::from(self.okb_path.trim());
                    let password = self.okb_password.take_secure();
                    let master_password = self.master_password.take_secure();
                    ctx.send_system_command(Command::RestoreDatabaseFromOkb {
                        path,
                        password,
                        master_password: Some(master_password),
                    });
                }
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.master_password.clear();
                self.error = None;
                self.mode = DatabaseRecoveryMode::OkbPasswordInput;
                ScreenResult::Continue
            }
            KeyCode::Char(c) => {
                self.master_password.push_char(c);
                self.error = None;
                ScreenResult::Continue
            }
            KeyCode::Backspace => {
                self.master_password.pop_char();
                self.error = None;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::DatabaseRestoreNeedsOAuth => {
                self.mode = DatabaseRecoveryMode::CloudNeedsOAuth;
                ScreenResult::Continue
            }
            CommandResult::DatabaseRestoreProgress {
                current,
                total,
                label,
            } => {
                self.progress = Some((current, total, label));
                ScreenResult::Continue
            }
            CommandResult::DatabaseRestored { source } => {
                self.mode = match source {
                    crate::commands::types::DatabaseRecoverySource::Cloud => {
                        DatabaseRecoveryMode::CloudSucceeded
                    }
                    crate::commands::types::DatabaseRecoverySource::Okb => {
                        DatabaseRecoveryMode::OkbSucceeded
                    }
                };
                ScreenResult::Continue
            }
            CommandResult::DatabaseValidationFailed { reason } => {
                self.error = Some(reason);
                ScreenResult::Continue
            }
            CommandResult::OAuth2Authorized { .. } => {
                // OAuth completed — re-trigger cloud restore
                if matches!(self.origin, DatabaseRecoveryOrigin::StartupKeyOnly) {
                    self.mode = DatabaseRecoveryMode::CloudMasterPasswordInput;
                    ScreenResult::Continue
                } else {
                    self.mode = DatabaseRecoveryMode::CloudSyncing;
                    ScreenResult::Command(Box::new(Command::RestoreDatabaseFromCloud {
                        master_password: None,
                    }))
                }
            }
            CommandResult::OAuth2Failed { error, .. } => {
                self.error = Some(error);
                self.mode = DatabaseRecoveryMode::SourceSelection;
                ScreenResult::Continue
            }
            CommandResult::Error { fallback, .. } => {
                self.error = Some(fallback);
                if matches!(self.mode, DatabaseRecoveryMode::CloudSyncing) {
                    self.mode = DatabaseRecoveryMode::CloudFailed;
                }
                ScreenResult::Continue
            }
            CommandResult::Cancelled { .. } => {
                self.mode = DatabaseRecoveryMode::SourceSelection;
                self.error = None;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
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
        match self.mode {
            DatabaseRecoveryMode::SourceSelection => self.handle_source_selection(key, &mut ctx),
            DatabaseRecoveryMode::OkbPathInput => self.handle_okb_input(key, &mut ctx),
            DatabaseRecoveryMode::OkbPasswordInput => self.handle_okb_password(key, &mut ctx),
            DatabaseRecoveryMode::OkbMasterPasswordInput => {
                self.handle_okb_master_password(key, &mut ctx)
            }
            DatabaseRecoveryMode::CloudMasterPasswordInput => {
                self.handle_cloud_master_password(key, &mut ctx)
            }
            _ => ScreenResult::Continue,
        }
    }
}

impl ScreenTrait for DatabaseRecoveryScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key)
                if key.kind == KeyEventKind::Press
                    && (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT) =>
            {
                match self.mode {
                    DatabaseRecoveryMode::SourceSelection => self.handle_source_selection(key, ctx),
                    DatabaseRecoveryMode::OkbPathInput => self.handle_okb_input(key, ctx),
                    DatabaseRecoveryMode::OkbPasswordInput => self.handle_okb_password(key, ctx),
                    DatabaseRecoveryMode::OkbMasterPasswordInput => {
                        self.handle_okb_master_password(key, ctx)
                    }
                    DatabaseRecoveryMode::CloudMasterPasswordInput => {
                        self.handle_cloud_master_password(key, ctx)
                    }
                    DatabaseRecoveryMode::CloudSyncing
                    | DatabaseRecoveryMode::CloudNeedsOAuth
                    | DatabaseRecoveryMode::CloudFailed
                    | DatabaseRecoveryMode::CloudSucceeded
                    | DatabaseRecoveryMode::OkbSucceeded => match key.code {
                        KeyCode::Esc => {
                            self.mode = DatabaseRecoveryMode::SourceSelection;
                            self.error = None;
                            ScreenResult::Continue
                        }
                        KeyCode::Enter
                            if matches!(self.mode, DatabaseRecoveryMode::CloudFailed) =>
                        {
                            self.error = None;
                            if matches!(self.origin, DatabaseRecoveryOrigin::StartupKeyOnly) {
                                self.mode = DatabaseRecoveryMode::CloudMasterPasswordInput;
                            } else {
                                self.mode = DatabaseRecoveryMode::CloudSyncing;
                                let _ =
                                    ctx.command_tx.try_send(Command::RestoreDatabaseFromCloud {
                                        master_password: None,
                                    });
                            }
                            ScreenResult::Continue
                        }
                        KeyCode::Enter
                            if matches!(self.mode, DatabaseRecoveryMode::CloudNeedsOAuth) =>
                        {
                            ctx.send_system_command(Command::OAuth2AuthorizeGoogleDrive);
                            ScreenResult::Continue
                        }
                        KeyCode::Enter
                            if matches!(self.mode, DatabaseRecoveryMode::CloudSucceeded) =>
                        {
                            ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
                        }
                        KeyCode::Enter
                            if matches!(self.mode, DatabaseRecoveryMode::OkbSucceeded) =>
                        {
                            ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
                        }
                        _ => ScreenResult::Continue,
                    },
                }
            }
            Message::CommandCompleted(result) => self.handle_command_result(result),
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let wide = WidthTier::from_width(area.width) != WidthTier::TooSmall;
        let use_onboarding_header =
            matches!(self.origin, DatabaseRecoveryOrigin::OnboardingRestore);
        let brand_rows = if use_onboarding_header {
            header_rows(wide)
        } else {
            1
        };
        let separator_rows = if use_onboarding_header { 0 } else { 1 };
        let content_area = Self::centered_content(area, 15 + brand_rows + separator_rows);

        let rows = Layout::vertical([
            Constraint::Length(brand_rows),     // logo or brand
            Constraint::Length(separator_rows), // separator for compact startup recovery
            Constraint::Length(2),              // title
            Constraint::Length(2),              // instruction
            Constraint::Length(6),              // content area
            Constraint::Length(1),              // error/hint
            Constraint::Length(1),              // hotkeys
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

        // Title + instruction vary by mode
        match self.mode {
            DatabaseRecoveryMode::SourceSelection => {
                self.render_source_selection(frame, &rows);
            }
            DatabaseRecoveryMode::OkbPathInput => {
                self.render_okb_input(frame, &rows);
            }
            DatabaseRecoveryMode::OkbPasswordInput => {
                self.render_okb_password(frame, &rows);
            }
            DatabaseRecoveryMode::OkbMasterPasswordInput => {
                self.render_master_password(frame, &rows, t!("tui.entry.db_recovery_okb_title"));
            }
            DatabaseRecoveryMode::CloudMasterPasswordInput => {
                self.render_master_password(frame, &rows, t!("tui.entry.db_recovery_cloud_title"));
            }
            DatabaseRecoveryMode::CloudSyncing => {
                self.render_cloud_syncing(frame, &rows);
            }
            DatabaseRecoveryMode::CloudNeedsOAuth => {
                self.render_cloud_needs_oauth(frame, &rows);
            }
            DatabaseRecoveryMode::CloudFailed => {
                self.render_cloud_failed(frame, &rows);
            }
            DatabaseRecoveryMode::CloudSucceeded => {
                self.render_cloud_succeeded(frame, &rows);
            }
            DatabaseRecoveryMode::OkbSucceeded => {
                self.render_okb_succeeded(frame, &rows);
            }
        }
    }

    fn on_mount(&mut self, _ctx: &mut ScreenContext) {}

    fn on_unmount(&mut self) {
        self.okb_password.clear();
        self.master_password.clear();
        self.okb_path.clear();
        self.error = None;
    }
}

// ── Rendering helpers ──────────────────────────────────────────────────────

impl DatabaseRecoveryScreen {
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

    fn render_source_selection(&self, frame: &mut ratatui::Frame, rows: &[ratatui::layout::Rect]) {
        let title = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_title"),
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let instruction = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_select_hint"),
            Style::default().fg(TEXT),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[3]);

        let cards_area = rows[4];
        let cards_layout = Layout::vertical([
            Constraint::Length(3), // cloud card
            Constraint::Length(1), // gap
            Constraint::Length(3), // okb card
        ])
        .split(cards_area);

        let cloud_style = if self.focus == DatabaseRecoveryFocus::Cloud {
            Style::default().fg(PRIMARY)
        } else {
            Style::default().fg(TEXT)
        };
        let cloud_card = Paragraph::new(vec![
            Line::from(Span::styled(
                t!("tui.entry.db_recovery_cloud_card_title"),
                cloud_style.add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                t!("tui.entry.db_recovery_cloud_card_desc"),
                Style::default().fg(TEXT_MUTED),
            )),
        ])
        .block(
            Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(if self.focus == DatabaseRecoveryFocus::Cloud {
                    cloud_style
                } else {
                    Style::default().fg(TEXT_MUTED)
                }),
        );
        frame.render_widget(cloud_card, cards_layout[0]);

        let okb_style = if self.focus == DatabaseRecoveryFocus::Okb {
            Style::default().fg(PRIMARY)
        } else {
            Style::default().fg(TEXT)
        };
        let okb_card = Paragraph::new(vec![
            Line::from(Span::styled(
                t!("tui.entry.db_recovery_okb_card_title"),
                okb_style.add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                t!("tui.entry.db_recovery_okb_card_desc"),
                Style::default().fg(TEXT_MUTED),
            )),
        ])
        .block(
            Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(if self.focus == DatabaseRecoveryFocus::Okb {
                    okb_style
                } else {
                    Style::default().fg(TEXT_MUTED)
                }),
        );
        frame.render_widget(okb_card, cards_layout[2]);

        // Hotkeys
        let hotkey = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_source_hotkey"),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        // Step
        let step_text = self.step_text();
        let step = Paragraph::new(Line::from(Span::styled(
            step_text.as_ref(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_okb_input(&self, frame: &mut ratatui::Frame, rows: &[ratatui::layout::Rect]) {
        let title = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_okb_title"),
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let instruction = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_okb_instruction"),
            Style::default().fg(TEXT),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[3]);

        let input_area = rows[4];
        let input_layout =
            Layout::vertical([Constraint::Length(3), Constraint::Length(1)]).split(input_area);

        let display = if self.okb_path.is_empty() {
            t!("tui.entry.db_recovery_okb_path_placeholder").to_string()
        } else {
            self.okb_path.clone()
        };
        let style = if self.okb_path.is_empty() {
            Style::default().fg(TEXT_MUTED)
        } else {
            Style::default().fg(TEXT)
        };
        let input = Paragraph::new(Line::from(Span::styled(display, style))).block(
            Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(PRIMARY))
                .title(t!("tui.entry.db_recovery_okb_path_label")),
        );
        frame.render_widget(input, input_layout[0]);

        // Error
        if let Some(ref err) = self.error {
            let error_line = Paragraph::new(Line::from(Span::styled(
                format!("✕ {}", err),
                Style::default().fg(ERROR),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(error_line, rows[5]);
        }

        // Hotkeys
        let hotkey = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_okb_hotkey"),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step_text = self.step_text();
        let step = Paragraph::new(Line::from(Span::styled(
            step_text.as_ref(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_okb_password(&self, frame: &mut ratatui::Frame, rows: &[ratatui::layout::Rect]) {
        let title = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_okb_title"),
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let instruction = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_okb_password_instruction"),
            Style::default().fg(TEXT),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[3]);

        let input_area = rows[4];
        let input_layout =
            Layout::vertical([Constraint::Length(3), Constraint::Length(1)]).split(input_area);

        let display = if self.okb_password.is_empty() {
            t!("tui.entry.db_recovery_okb_password_placeholder").to_string()
        } else {
            "•".repeat(self.okb_password.len())
        };
        let style = if self.okb_password.is_empty() {
            Style::default().fg(TEXT_MUTED)
        } else {
            Style::default().fg(TEXT)
        };
        let input = Paragraph::new(Line::from(Span::styled(display, style))).block(
            Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(PRIMARY))
                .title(t!("tui.entry.db_recovery_okb_password_label")),
        );
        frame.render_widget(input, input_layout[0]);

        // Error
        if let Some(ref err) = self.error {
            let error_line = Paragraph::new(Line::from(Span::styled(
                format!("✕ {}", err),
                Style::default().fg(ERROR),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(error_line, rows[5]);
        }

        // Hotkeys
        let hotkey = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_okb_password_hotkey"),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step_text = self.step_text();
        let step = Paragraph::new(Line::from(Span::styled(
            step_text.as_ref(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_master_password(
        &self,
        frame: &mut ratatui::Frame,
        rows: &[ratatui::layout::Rect],
        title_text: std::borrow::Cow<'static, str>,
    ) {
        let title = Paragraph::new(Line::from(Span::styled(
            title_text,
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let instruction = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_master_password_title"),
            Style::default().fg(TEXT),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[3]);

        let input_area = rows[4];
        let input_layout =
            Layout::vertical([Constraint::Length(3), Constraint::Length(1)]).split(input_area);

        let display = if self.master_password.is_empty() {
            t!("tui.entry.db_recovery_master_password_placeholder").to_string()
        } else {
            "•".repeat(self.master_password.len())
        };
        let style = if self.master_password.is_empty() {
            Style::default().fg(TEXT_MUTED)
        } else {
            Style::default().fg(TEXT)
        };
        let input = Paragraph::new(Line::from(Span::styled(display, style))).block(
            Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(PRIMARY))
                .title(t!("tui.entry.db_recovery_master_password_title")),
        );
        frame.render_widget(input, input_layout[0]);

        if let Some(ref err) = self.error {
            let error_line = Paragraph::new(Line::from(Span::styled(
                format!("✕ {}", err),
                Style::default().fg(ERROR),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(error_line, rows[5]);
        }

        let hotkey = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_okb_password_hotkey"),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step_text = self.step_text();
        let step = Paragraph::new(Line::from(Span::styled(
            step_text.as_ref(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_cloud_syncing(&self, frame: &mut ratatui::Frame, rows: &[ratatui::layout::Rect]) {
        let title = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_cloud_title"),
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let content = if let Some((current, total, ref label)) = self.progress {
            vec![
                Line::from(Span::styled(
                    t!("tui.entry.db_recovery_cloud_syncing"),
                    Style::default().fg(TEXT),
                )),
                Line::from(Span::styled("", Style::default().fg(TEXT))),
                Line::from(Span::styled(
                    format!("→ {}", label),
                    Style::default().fg(TEXT_MUTED),
                )),
                Line::from(Span::styled(
                    format!(
                        "  {}/{} ({:.0}%)",
                        current,
                        total,
                        if total > 0 {
                            (current as f64 / total as f64) * 100.0
                        } else {
                            0.0
                        }
                    ),
                    Style::default().fg(PRIMARY),
                )),
            ]
        } else {
            vec![
                Line::from(Span::styled(
                    t!("tui.entry.db_recovery_cloud_syncing"),
                    Style::default().fg(TEXT),
                )),
                Line::from(Span::styled("", Style::default().fg(TEXT))),
                Line::from(Span::styled(
                    t!("tui.entry.db_recovery_cloud_oauth_found"),
                    Style::default().fg(SUCCESS),
                )),
                Line::from(Span::styled(
                    t!("tui.entry.db_recovery_cloud_downloading"),
                    Style::default().fg(TEXT_MUTED),
                )),
            ]
        };
        let para = Paragraph::new(content).alignment(Alignment::Center);
        let combined = ratatui::layout::Rect {
            y: rows[3].y,
            height: rows[4].y + rows[4].height - rows[3].y,
            ..rows[3]
        };
        frame.render_widget(para, combined);

        // Hotkeys
        let hotkey = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_cloud_cancel_hint"),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step_text = self.step_text();
        let step = Paragraph::new(Line::from(Span::styled(
            step_text.as_ref(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_cloud_needs_oauth(&self, frame: &mut ratatui::Frame, rows: &[ratatui::layout::Rect]) {
        let title = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_cloud_title"),
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let instruction = Paragraph::new(vec![Line::from(Span::styled(
            t!("tui.entry.db_recovery_cloud_oauth_needed"),
            Style::default().fg(TEXT),
        ))])
        .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[3]);

        let card_area = rows[4];
        let card = Paragraph::new(vec![
            Line::from(Span::styled(
                t!("tui.entry.db_recovery_cloud_oauth_card_title"),
                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                t!("tui.entry.db_recovery_cloud_oauth_card_desc"),
                Style::default().fg(TEXT_MUTED),
            )),
        ])
        .block(
            Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(PRIMARY)),
        );
        frame.render_widget(card, card_area);

        let hotkey = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_cloud_oauth_hotkey"),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step_text = self.step_text();
        let step = Paragraph::new(Line::from(Span::styled(
            step_text.as_ref(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_cloud_failed(&self, frame: &mut ratatui::Frame, rows: &[ratatui::layout::Rect]) {
        let title = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_cloud_title"),
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let default_failed = t!("tui.entry.db_recovery_cloud_failed_default");
        let error_text = self.error.as_deref().unwrap_or(default_failed.as_ref());
        let content = vec![
            Line::from(Span::styled(
                format!("✕ {}", error_text),
                Style::default().fg(ERROR),
            )),
            Line::from(Span::styled("", Style::default().fg(TEXT))),
            Line::from(Span::styled(
                t!("tui.entry.db_recovery_cloud_failed_hint"),
                Style::default().fg(TEXT_MUTED),
            )),
        ];
        let para = Paragraph::new(content).alignment(Alignment::Center);
        let combined = ratatui::layout::Rect {
            y: rows[3].y,
            height: rows[4].y + rows[4].height - rows[3].y,
            ..rows[3]
        };
        frame.render_widget(para, combined);

        let hotkey = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_cloud_failed_hotkey"),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step_text = self.step_text();
        let step = Paragraph::new(Line::from(Span::styled(
            step_text.as_ref(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_cloud_succeeded(&self, frame: &mut ratatui::Frame, rows: &[ratatui::layout::Rect]) {
        let title = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_cloud_title"),
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let content = vec![
            Line::from(Span::styled(
                t!("tui.entry.db_recovery_cloud_success_downloaded"),
                Style::default().fg(SUCCESS),
            )),
            Line::from(Span::styled(
                t!("tui.entry.db_recovery_cloud_success_verified"),
                Style::default().fg(SUCCESS),
            )),
        ];
        let para = Paragraph::new(content).alignment(Alignment::Center);
        let combined = ratatui::layout::Rect {
            y: rows[3].y,
            height: rows[4].y + rows[4].height - rows[3].y,
            ..rows[3]
        };
        frame.render_widget(para, combined);

        let hotkey = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_cloud_success_hotkey"),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step_text = self.step_text();
        let step = Paragraph::new(Line::from(Span::styled(
            step_text.as_ref(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_okb_succeeded(&self, frame: &mut ratatui::Frame, rows: &[ratatui::layout::Rect]) {
        let title = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_okb_succeeded_title"),
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let content = vec![Line::from(Span::styled(
            t!("tui.entry.db_recovery_cloud_success_verified"),
            Style::default().fg(SUCCESS),
        ))];
        let para = Paragraph::new(content).alignment(Alignment::Center);
        let combined = ratatui::layout::Rect {
            y: rows[3].y,
            height: rows[4].y + rows[4].height - rows[3].y,
            ..rows[3]
        };
        frame.render_widget(para, combined);

        let hotkey = Paragraph::new(Line::from(Span::styled(
            t!("tui.entry.db_recovery_cloud_success_hotkey"),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step_text = self.step_text();
        let step = Paragraph::new(Line::from(Span::styled(
            step_text.as_ref(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::traits::screen::{Screen, ScreenContext};
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

    fn char_key(c: char) -> KeyEvent {
        key(KeyCode::Char(c))
    }

    fn context<'a>(
        tx: &'a tokio::sync::mpsc::Sender<Command>,
        config: &'a crate::config::AppConfig,
    ) -> ScreenContext<'a> {
        ScreenContext {
            command_tx: tx,
            config,
        }
    }

    fn render_database_recovery_buffer(
        screen: &DatabaseRecoveryScreen,
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
    fn starts_on_source_selection() {
        let screen = DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::OnboardingRestore);
        assert_eq!(screen.mode, DatabaseRecoveryMode::SourceSelection);
        assert_eq!(screen.focus, DatabaseRecoveryFocus::Cloud);
    }

    #[test]
    fn onboarding_restore_renders_ascii_logo_on_tall_terminal() {
        let screen = DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::OnboardingRestore);

        let buffer = render_database_recovery_buffer(&screen, 80, 24);

        assert!(format!("{buffer:?}").contains("░█▀█"));
    }

    #[test]
    fn down_moves_focus_to_okb() {
        let mut screen = DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly);
        screen.handle_key_for_test(key(KeyCode::Down));
        assert_eq!(screen.focus, DatabaseRecoveryFocus::Okb);
    }

    #[test]
    fn enter_on_okb_without_path_sets_error() {
        let mut screen = DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly);
        screen.focus = DatabaseRecoveryFocus::Okb;
        screen.mode = DatabaseRecoveryMode::OkbPathInput;
        screen.handle_key_for_test(key(KeyCode::Enter));
        let expected = t!("tui.entry.db_recovery_okb_empty_error").to_string();
        assert_eq!(screen.error.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn startup_cloud_requires_master_password_before_restore_command() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let config = crate::config::AppConfig::default();
        let mut ctx = context(&tx, &config);
        let mut screen = DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly);

        let result = screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);

        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.mode, DatabaseRecoveryMode::CloudMasterPasswordInput);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn startup_cloud_master_password_sends_restore_command_with_password() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let config = crate::config::AppConfig::default();
        let mut ctx = context(&tx, &config);
        let mut screen = DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly);

        screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);
        for c in "master-secret".chars() {
            screen.update(Message::KeyEvent(char_key(c)), &mut ctx);
        }
        screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);

        match rx.try_recv().expect("restore command") {
            Command::RestoreDatabaseFromCloud { master_password } => {
                let master_password = master_password.expect("startup restore sends password");
                assert_eq!(master_password.expose(), "master-secret");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn onboarding_cloud_restore_sends_none_immediately() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let config = crate::config::AppConfig::default();
        let mut ctx = context(&tx, &config);
        let mut screen = DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::OnboardingRestore);

        screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);

        assert_eq!(screen.mode, DatabaseRecoveryMode::CloudSyncing);
        match rx.try_recv().expect("restore command") {
            Command::RestoreDatabaseFromCloud { master_password } => {
                assert!(master_password.is_none());
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn startup_okb_collects_master_password_after_backup_password() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let config = crate::config::AppConfig::default();
        let mut ctx = context(&tx, &config);
        let mut screen = DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::StartupKeyOnly);
        screen.focus = DatabaseRecoveryFocus::Okb;

        screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);
        for c in "/tmp/backup.okb".chars() {
            screen.update(Message::KeyEvent(char_key(c)), &mut ctx);
        }
        screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);
        for c in "backup-secret".chars() {
            screen.update(Message::KeyEvent(char_key(c)), &mut ctx);
        }
        screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);

        assert_eq!(screen.mode, DatabaseRecoveryMode::OkbMasterPasswordInput);
        assert!(rx.try_recv().is_err());

        for c in "master-secret".chars() {
            screen.update(Message::KeyEvent(char_key(c)), &mut ctx);
        }
        screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);

        match rx.try_recv().expect("restore command") {
            Command::RestoreDatabaseFromOkb {
                path,
                password,
                master_password,
            } => {
                assert_eq!(path, std::path::PathBuf::from("/tmp/backup.okb"));
                assert_eq!(password.expose(), "backup-secret");
                let master_password = master_password.expect("startup restore sends password");
                assert_eq!(master_password.expose(), "master-secret");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn onboarding_okb_restore_sends_none_after_backup_password() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let config = crate::config::AppConfig::default();
        let mut ctx = context(&tx, &config);
        let mut screen = DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::OnboardingRestore);
        screen.focus = DatabaseRecoveryFocus::Okb;

        screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);
        for c in "/tmp/backup.okb".chars() {
            screen.update(Message::KeyEvent(char_key(c)), &mut ctx);
        }
        screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);
        for c in "backup-secret".chars() {
            screen.update(Message::KeyEvent(char_key(c)), &mut ctx);
        }
        screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);

        match rx.try_recv().expect("restore command") {
            Command::RestoreDatabaseFromOkb {
                path,
                password,
                master_password,
            } => {
                assert_eq!(path, std::path::PathBuf::from("/tmp/backup.okb"));
                assert_eq!(password.expose(), "backup-secret");
                assert!(master_password.is_none());
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
