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
use crate::tui::theme::{self, BRAND, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED};
use crate::tui::traits::screen::{Screen as ScreenTrait, ScreenContext, ScreenResult};

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
    CloudSyncing,
    CloudNeedsOAuth,
    CloudFailed,
    CloudSucceeded,
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
            error: None,
            progress: None,
        }
    }

    fn step_text(&self) -> &'static str {
        match self.origin {
            DatabaseRecoveryOrigin::StartupKeyOnly => "Step 1/1",
            DatabaseRecoveryOrigin::OnboardingRestore => "Step 3/3",
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
                self.mode = DatabaseRecoveryMode::CloudSyncing;
                let _ = ctx.command_tx.try_send(Command::RestoreDatabaseFromCloud);
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

    fn handle_okb_input(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Enter => {
                let path = self.okb_path.trim();
                if path.is_empty() {
                    self.error = Some("Enter a .okb path.".to_string());
                    ScreenResult::Continue
                } else if !path.ends_with(".okb") {
                    self.error = Some("Path must end with .okb.".to_string());
                    ScreenResult::Continue
                } else {
                    self.error = None;
                    let _ = ctx.command_tx.try_send(Command::RestoreDatabaseFromOkb {
                        path: std::path::PathBuf::from(path),
                    });
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

    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::DatabaseRestoreNeedsOAuth => {
                self.mode = DatabaseRecoveryMode::CloudNeedsOAuth;
                ScreenResult::Continue
            }
            CommandResult::DatabaseRestoreProgress { current, total, label } => {
                self.progress = Some((current, total, label));
                ScreenResult::Continue
            }
            CommandResult::DatabaseRestored { .. } => {
                self.mode = DatabaseRecoveryMode::CloudSucceeded;
                ScreenResult::Continue
            }
            CommandResult::DatabaseValidationFailed { reason } => {
                self.error = Some(reason);
                ScreenResult::Continue
            }
            CommandResult::Error { fallback, .. } => {
                self.error = Some(fallback);
                if matches!(self.mode, DatabaseRecoveryMode::CloudSyncing) {
                    self.mode = DatabaseRecoveryMode::CloudFailed;
                }
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
                    DatabaseRecoveryMode::SourceSelection => {
                        self.handle_source_selection(key, ctx)
                    }
                    DatabaseRecoveryMode::OkbPathInput => self.handle_okb_input(key, ctx),
                    DatabaseRecoveryMode::CloudSyncing
                    | DatabaseRecoveryMode::CloudNeedsOAuth
                    | DatabaseRecoveryMode::CloudFailed
                    | DatabaseRecoveryMode::CloudSucceeded => match key.code {
                        KeyCode::Esc => {
                            self.mode = DatabaseRecoveryMode::SourceSelection;
                            self.error = None;
                            ScreenResult::Continue
                        }
                        KeyCode::Enter
                            if matches!(self.mode, DatabaseRecoveryMode::CloudFailed) =>
                        {
                            self.mode = DatabaseRecoveryMode::CloudSyncing;
                            self.error = None;
                            let _ = ctx
                                .command_tx
                                .try_send(Command::RestoreDatabaseFromCloud);
                            ScreenResult::Continue
                        }
                        KeyCode::Enter
                            if matches!(
                                self.mode,
                                DatabaseRecoveryMode::CloudNeedsOAuth
                            ) =>
                        {
                            let _ = ctx
                                .command_tx
                                .try_send(Command::OAuth2AuthorizeGoogleDrive);
                            ScreenResult::Continue
                        }
                        KeyCode::Enter
                            if matches!(
                                self.mode,
                                DatabaseRecoveryMode::CloudSucceeded
                            ) =>
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
        let content_area = Self::centered_content(area, 17);

        let rows = Layout::vertical([
            Constraint::Length(1),  // brand
            Constraint::Length(1),  // separator
            Constraint::Length(2),  // title
            Constraint::Length(2),  // instruction
            Constraint::Length(6),  // content area
            Constraint::Length(1),  // error/hint
            Constraint::Length(1),  // hotkeys
            Constraint::Length(1),  // step
        ])
        .split(content_area);

        // Brand
        let brand = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} ", theme::ICON_LOCK),
                Style::default().fg(BRAND),
            ),
            Span::styled("OpenKeyring", Style::default().fg(BRAND).add_modifier(Modifier::BOLD)),
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

        // Title + instruction vary by mode
        match self.mode {
            DatabaseRecoveryMode::SourceSelection => {
                self.render_source_selection(frame, &rows);
            }
            DatabaseRecoveryMode::OkbPathInput => {
                self.render_okb_input(frame, &rows);
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
        }
    }

    fn on_mount(&mut self, _ctx: &mut ScreenContext) {}

    fn on_unmount(&mut self) {}
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

    fn render_source_selection(
        &self,
        frame: &mut ratatui::Frame,
        rows: &[ratatui::layout::Rect],
    ) {
        let title = Paragraph::new(Line::from(Span::styled(
            "恢复 vault 数据库",
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let instruction = Paragraph::new(Line::from(Span::styled(
            "选择加密数据库来源。恢复完成后会用当前 key 解密验证。",
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
            Line::from(Span::styled("↻  云端同步恢复", cloud_style.add_modifier(Modifier::BOLD))),
            Line::from(Span::styled(
                "    使用已配置的 OAuth；没有配置则先授权。",
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
            Line::from(Span::styled("↓  从 .okb 备份恢复", okb_style.add_modifier(Modifier::BOLD))),
            Line::from(Span::styled(
                "    输入本地 .okb 文件路径。",
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
            "↑↓/Tab: navigate  |  Enter: select  |  Esc: back",
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        // Step
        let step = Paragraph::new(Line::from(Span::styled(
            self.step_text(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_okb_input(&self, frame: &mut ratatui::Frame, rows: &[ratatui::layout::Rect]) {
        let title = Paragraph::new(Line::from(Span::styled(
            "从 .okb 备份恢复",
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let instruction = Paragraph::new(Line::from(Span::styled(
            "输入本地 .okb 文件路径，提交后检查文件存在性和格式。",
            Style::default().fg(TEXT),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[3]);

        let input_area = rows[4];
        let input_layout = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(input_area);

        let display = if self.okb_path.is_empty() {
            "/path/to/backup.okb".to_string()
        } else {
            self.okb_path.clone()
        };
        let style = if self.okb_path.is_empty() {
            Style::default().fg(TEXT_MUTED)
        } else {
            Style::default().fg(TEXT)
        };
        let input = Paragraph::new(Line::from(Span::styled(display, style)))
            .block(
                Block::default()
                    .borders(ratatui::widgets::Borders::ALL)
                    .border_style(Style::default().fg(PRIMARY))
                    .title(" .okb path "),
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
            "Enter: restore .okb  |  Esc: source selection",
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step = Paragraph::new(Line::from(Span::styled(
            self.step_text(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_cloud_syncing(&self, frame: &mut ratatui::Frame, rows: &[ratatui::layout::Rect]) {
        let title = Paragraph::new(Line::from(Span::styled(
            "云端同步恢复",
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let content = if let Some((current, total, ref label)) = self.progress {
            vec![
                Line::from(Span::styled(
                    "正在从云端同步加密 vault.db。",
                    Style::default().fg(TEXT),
                )),
                Line::from(Span::styled("", Style::default().fg(TEXT))),
                Line::from(Span::styled(
                    format!("→ {}", label),
                    Style::default().fg(TEXT_MUTED),
                )),
                Line::from(Span::styled(
                    format!("  {}/{} ({:.0}%)", current, total,
                        if total > 0 { (current as f64 / total as f64) * 100.0 } else { 0.0 }
                    ),
                    Style::default().fg(PRIMARY),
                )),
            ]
        } else {
            vec![
                Line::from(Span::styled(
                    "正在从云端同步加密 vault.db。",
                    Style::default().fg(TEXT),
                )),
                Line::from(Span::styled("", Style::default().fg(TEXT))),
                Line::from(Span::styled(
                    "✓ OAuth 配置已找到",
                    Style::default().fg(SUCCESS),
                )),
                Line::from(Span::styled(
                    "→ 正在下载 vault.db...",
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
            "Esc: cancel and return to source selection",
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step = Paragraph::new(Line::from(Span::styled(
            self.step_text(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_cloud_needs_oauth(
        &self,
        frame: &mut ratatui::Frame,
        rows: &[ratatui::layout::Rect],
    ) {
        let title = Paragraph::new(Line::from(Span::styled(
            "云端同步恢复",
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let instruction = Paragraph::new(vec![
            Line::from(Span::styled(
                "需要先授权云端同步，授权完成后会自动下载 vault.db。",
                Style::default().fg(TEXT),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[3]);

        let card_area = rows[4];
        let card = Paragraph::new(vec![
            Line::from(Span::styled(
                "↗  开始 OAuth 授权",
                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "    授权只用于读取 oak-keyring 同步数据。",
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
            "Enter: authorize  |  Esc: source selection",
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step = Paragraph::new(Line::from(Span::styled(
            self.step_text(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_cloud_failed(&self, frame: &mut ratatui::Frame, rows: &[ratatui::layout::Rect]) {
        let title = Paragraph::new(Line::from(Span::styled(
            "云端同步恢复",
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let error_text = self
            .error
            .as_deref()
            .unwrap_or("没有找到可恢复的 vault.db，或同步失败。");
        let content = vec![
            Line::from(Span::styled(
                format!("✕ {}", error_text),
                Style::default().fg(ERROR),
            )),
            Line::from(Span::styled("", Style::default().fg(TEXT))),
            Line::from(Span::styled(
                "你可以重试云端同步，或返回选择 .okb 备份。",
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
            "Enter: retry cloud sync  |  Esc: source selection",
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step = Paragraph::new(Line::from(Span::styled(
            self.step_text(),
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(step, rows[7]);
    }

    fn render_cloud_succeeded(
        &self,
        frame: &mut ratatui::Frame,
        rows: &[ratatui::layout::Rect],
    ) {
        let title = Paragraph::new(Line::from(Span::styled(
            "云端同步恢复",
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[2]);

        let content = vec![
            Line::from(Span::styled(
                "✓ 已下载 vault.db",
                Style::default().fg(SUCCESS),
            )),
            Line::from(Span::styled(
                "✓ 已通过当前 vault key 解密验证",
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
            "Enter: continue",
            Style::default().fg(TEXT_MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hotkey, rows[6]);

        let step = Paragraph::new(Line::from(Span::styled(
            self.step_text(),
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

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn starts_on_source_selection() {
        let screen = DatabaseRecoveryScreen::new(DatabaseRecoveryOrigin::OnboardingRestore);
        assert_eq!(screen.mode, DatabaseRecoveryMode::SourceSelection);
        assert_eq!(screen.focus, DatabaseRecoveryFocus::Cloud);
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
        assert_eq!(screen.error.as_deref(), Some("Enter a .okb path."));
    }
}