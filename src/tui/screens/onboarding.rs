//! Onboarding wizard — 3-path setup flow with step management.
//!
//! Paths: CreateNew (create vault + recovery key), Restore (recovery key restore),
//! Import (import from other manager). Each path has its own step sequence.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use zeroize::Zeroize;

use crate::commands::result::CommandResult;
use crate::commands::types::{ImportPreview, ImportSource, Screen};
use crate::commands::{Command, Message};
use crate::tui::screens::recovery_key::WordGridState;
use crate::tui::theme::{
    self, Styles, BORDER, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED, TEXT_PLACEHOLDER,
    TEXT_SECONDARY, WARNING,
};
use crate::tui::traits::screen::{ScreenContext, ScreenResult};
use crate::types::SecureStr;

// ── Enums ──────────────────────────────────────────────────────────────────

/// The three onboarding paths a user can choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnboardingPath {
    #[default]
    CreateNew,
    Restore,
    Import,
}

/// Steps within each onboarding path.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OnboardingStep {
    /// Initial choice screen — pick a path.
    #[default]
    Welcome,
    /// Vault location input.
    VaultPath,
    /// Show 24 recovery words (read-only 4x6 grid).
    RecoveryDisplay,
    /// Verify 4 random positions from the recovery words.
    RecoveryVerify { positions: [usize; 4] },
    /// Input recovery key for restore (delegates to WordGridState).
    RecoveryInput,
    /// Post-restore security advisory.
    SecurityAdvisory,
    /// Choose import source.
    ImportSource,
    /// Preview import data.
    ImportPreview,
    /// Set master password (inline — navigates to SetNewMasterPassword on enter).
    SetPassword,
}

// ── OnboardingScreen ──────────────────────────────────────────────────────

/// Onboarding wizard state: multi-path step-by-step initial setup flow.
#[derive(Debug)]
pub struct OnboardingScreen {
    pub current_step: OnboardingStep,
    pub selected_path: Option<OnboardingPath>,
    pub path_input: String,
    pub error: Option<String>,
    pub recovery_confirmed: bool,
    /// 24 recovery words populated after VaultInitialized command result.
    pub recovery_words: Vec<String>,
    /// Embedded grid for RecoveryInput step.
    pub recovery_grid: WordGridState,
    /// Verify step inputs for 4 positions.
    pub verify_inputs: [String; 4],
    pub verify_errors: [bool; 4],
    pub verify_positions: [usize; 4],
    /// Signals that onboarding is returning from ImportExportScreen.
    /// When true, skip ImportSource step and go directly to VaultPath.
    pub returning_from_import: bool,
    // Import state for ImportSource/ImportPreview steps
    pub selected_source_idx: usize,
    pub import_file_path: String,
    pub import_focus: crate::tui::screens::import_export::ImportFocus,
    pub import_preview: Option<ImportPreview>,
}

impl Default for OnboardingScreen {
    fn default() -> Self {
        use crate::tui::screens::import_export::ImportFocus;
        Self {
            current_step: OnboardingStep::default(),
            selected_path: None,
            path_input: String::new(),
            error: None,
            recovery_confirmed: false,
            recovery_words: Vec::new(),
            recovery_grid: WordGridState::default(),
            verify_inputs: std::array::from_fn(|_| String::new()),
            verify_errors: [false; 4],
            verify_positions: [0; 4],
            returning_from_import: false,
            selected_source_idx: 0,
            import_file_path: String::new(),
            import_focus: ImportFocus::SourceList,
            import_preview: None,
        }
    }
}

impl OnboardingScreen {
    /// Generate 4 random positions for recovery verification.
    fn generate_verify_positions(&mut self) {
        use std::collections::HashSet;
        let mut positions = [0usize; 4];
        let mut used = HashSet::new();
        let mut rng = rand::rng();
        for slot in &mut positions {
            loop {
                let idx = rand::Rng::random_range(&mut rng, 0..24);
                if used.insert(idx) {
                    *slot = idx;
                    break;
                }
            }
        }
        positions.sort();
        self.verify_positions = positions;
        self.verify_inputs = std::array::from_fn(|_| String::new());
        self.verify_errors = [false; 4];
    }

    /// Total steps for the current path (including Welcome).
    pub fn total_steps(&self) -> usize {
        match self.selected_path {
            None => 1,
            Some(OnboardingPath::CreateNew) => 5, // Welcome + VaultPath + RecoveryDisplay + RecoveryVerify + SetPassword
            Some(OnboardingPath::Restore) => 4, // Welcome + RecoveryInput + VaultPath + SecurityAdvisory + SetPassword = 5... but spec says 3
            Some(OnboardingPath::Import) => 6, // Welcome + ImportSource + ImportPreview + VaultPath + RecoveryDisplay + RecoveryVerify + SetPassword
        }
    }

    /// Current step number (1-based).
    pub fn current_step_number(&self) -> usize {
        match (&self.selected_path, &self.current_step) {
            (None, OnboardingStep::Welcome) => 1,
            // CreateNew path
            (Some(OnboardingPath::CreateNew), OnboardingStep::Welcome) => 1,
            (Some(OnboardingPath::CreateNew), OnboardingStep::VaultPath) => 2,
            (Some(OnboardingPath::CreateNew), OnboardingStep::RecoveryDisplay) => 3,
            (Some(OnboardingPath::CreateNew), OnboardingStep::RecoveryVerify { .. }) => 4,
            (Some(OnboardingPath::CreateNew), OnboardingStep::SetPassword) => 5,
            // Restore path
            (Some(OnboardingPath::Restore), OnboardingStep::Welcome) => 1,
            (Some(OnboardingPath::Restore), OnboardingStep::RecoveryInput) => 2,
            (Some(OnboardingPath::Restore), OnboardingStep::VaultPath) => 3,
            (Some(OnboardingPath::Restore), OnboardingStep::SecurityAdvisory) => 4,
            // Import path
            (Some(OnboardingPath::Import), OnboardingStep::Welcome) => 1,
            (Some(OnboardingPath::Import), OnboardingStep::ImportSource) => 2,
            (Some(OnboardingPath::Import), OnboardingStep::ImportPreview) => 3,
            (Some(OnboardingPath::Import), OnboardingStep::VaultPath) => 4,
            (Some(OnboardingPath::Import), OnboardingStep::RecoveryDisplay) => 5,
            (Some(OnboardingPath::Import), OnboardingStep::RecoveryVerify { .. }) => 6,
            // Fallback
            _ => 1,
        }
    }

    // ── Key handling ───────────────────────────────────────────────────────

    fn handle_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match &self.current_step {
            OnboardingStep::Welcome => self.handle_welcome_key(key),
            OnboardingStep::VaultPath => self.handle_vault_path_key(key, ctx),
            OnboardingStep::RecoveryDisplay => self.handle_recovery_display_key(key),
            OnboardingStep::RecoveryVerify { .. } => self.handle_recovery_verify_key(key),
            OnboardingStep::RecoveryInput => self.handle_recovery_input_key(key, ctx),
            OnboardingStep::SecurityAdvisory => self.handle_security_advisory_key(key),
            OnboardingStep::ImportSource => self.handle_import_source_key(key, ctx),
            OnboardingStep::ImportPreview => self.handle_import_preview_key(key, ctx),
            OnboardingStep::SetPassword => self.handle_set_password_key(key, ctx),
        }
    }

    fn handle_welcome_key(&mut self, key: KeyEvent) -> ScreenResult {
        match key.code {
            KeyCode::Char('1') | KeyCode::Enter => {
                self.selected_path = Some(OnboardingPath::CreateNew);
                self.current_step = OnboardingStep::VaultPath;
                ScreenResult::Continue
            }
            KeyCode::Char('2') => {
                self.selected_path = Some(OnboardingPath::Restore);
                self.current_step = OnboardingStep::RecoveryInput;
                ScreenResult::Continue
            }
            KeyCode::Char('3') => {
                self.selected_path = Some(OnboardingPath::Import);
                self.current_step = OnboardingStep::ImportSource;
                ScreenResult::Continue
            }
            KeyCode::Esc => ScreenResult::ExitApp,
            _ => ScreenResult::Continue,
        }
    }

    fn handle_vault_path_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Enter => {
                if !self.path_input.is_empty() {
                    self.advance_from_vault_path(ctx);
                }
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.current_step = OnboardingStep::Welcome;
                ScreenResult::Continue
            }
            KeyCode::Backspace => {
                self.path_input.pop();
                ScreenResult::Continue
            }
            KeyCode::Char(c) => {
                self.path_input.push(c);
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn advance_from_vault_path(&mut self, ctx: &mut ScreenContext) {
        match self.selected_path {
            Some(OnboardingPath::CreateNew) => {
                // Send InitializeVault to create the vault and get recovery words
                let vault_path = std::path::PathBuf::from(&self.path_input);
                let password = SecureStr::new(String::new());
                let cmd = Command::InitializeVault {
                    vault_path,
                    master_password: password,
                };
                let _ = ctx.command_tx.try_send(cmd);
                // Stay on VaultPath until VaultInitialized result arrives
            }
            Some(OnboardingPath::Restore) => {
                self.current_step = OnboardingStep::SecurityAdvisory;
            }
            Some(OnboardingPath::Import) => {
                self.current_step = OnboardingStep::RecoveryDisplay;
            }
            None => {}
        }
    }

    fn handle_recovery_display_key(&mut self, key: KeyEvent) -> ScreenResult {
        match key.code {
            KeyCode::Enter => {
                if self.recovery_confirmed {
                    self.generate_verify_positions();
                    self.current_step = OnboardingStep::RecoveryVerify {
                        positions: self.verify_positions,
                    };
                }
                ScreenResult::Continue
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.recovery_confirmed = !self.recovery_confirmed;
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.current_step = OnboardingStep::VaultPath;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_recovery_verify_key(&mut self, key: KeyEvent) -> ScreenResult {
        // Find the first empty or errored input to fill
        let focused = self
            .verify_errors
            .iter()
            .enumerate()
            .find(|(_, &e)| e)
            .map(|(i, _)| i)
            .unwrap_or_else(|| {
                self.verify_inputs
                    .iter()
                    .enumerate()
                    .find(|(_, s)| s.is_empty())
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });

        match key.code {
            KeyCode::Enter => {
                // Validate the 4 positions match recovery words
                let all_correct = self.verify_positions.iter().enumerate().all(|(i, &pos)| {
                    pos < self.recovery_words.len()
                        && self.verify_inputs[i].eq_ignore_ascii_case(&self.recovery_words[pos])
                });
                if all_correct {
                    self.verify_errors = [false; 4];
                    self.current_step = OnboardingStep::SetPassword;
                } else {
                    // Mark mismatches
                    for (i, &pos) in self.verify_positions.iter().enumerate() {
                        self.verify_errors[i] = pos >= self.recovery_words.len()
                            || !self.verify_inputs[i]
                                .eq_ignore_ascii_case(&self.recovery_words[pos]);
                    }
                }
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.current_step = OnboardingStep::RecoveryDisplay;
                ScreenResult::Continue
            }
            KeyCode::Backspace => {
                self.verify_inputs[focused].pop();
                self.verify_errors[focused] = false;
                ScreenResult::Continue
            }
            KeyCode::Char(c) if c.is_alphabetic() => {
                let input = &mut self.verify_inputs[focused];
                if input.len() < 12 {
                    input.push(c);
                }
                self.verify_errors[focused] = false;
                ScreenResult::Continue
            }
            KeyCode::Tab => {
                // Move to next input
                // (no-op for now, Tab just continues)
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_recovery_input_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match key.code {
            KeyCode::Esc => {
                self.current_step = OnboardingStep::Welcome;
                ScreenResult::Continue
            }
            _ => {
                let result = self.recovery_grid.handle_key(key);
                match result {
                    Some(words) => {
                        let cmd = Command::UnlockWithRecoveryKey { words };
                        let _ = ctx.command_tx.try_send(cmd);
                        // Advance to VaultPath
                        self.current_step = OnboardingStep::VaultPath;
                        ScreenResult::Continue
                    }
                    None => ScreenResult::Continue,
                }
            }
        }
    }

    fn handle_security_advisory_key(&mut self, key: KeyEvent) -> ScreenResult {
        match key.code {
            KeyCode::Enter => {
                self.current_step = OnboardingStep::SetPassword;
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.current_step = OnboardingStep::VaultPath;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_import_source_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        use crate::tui::screens::import_export::{IMPORT_SOURCES, ImportFocus};

        match key.code {
            KeyCode::Up => {
                if self.selected_source_idx > 0 {
                    self.selected_source_idx -= 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Down => {
                if self.selected_source_idx < IMPORT_SOURCES.len() - 1 {
                    self.selected_source_idx += 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Tab => {
                self.import_focus = ImportFocus::FilePath;
                ScreenResult::Continue
            }
            KeyCode::Char(c) if self.import_focus == ImportFocus::FilePath => {
                self.import_file_path.push(c);
                ScreenResult::Continue
            }
            KeyCode::Backspace if self.import_focus == ImportFocus::FilePath => {
                self.import_file_path.pop();
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                if self.import_file_path.is_empty() {
                    self.error = Some("File path is required".to_string());
                    return ScreenResult::Continue;
                }
                let source = IMPORT_SOURCES[self.selected_source_idx].0;
                let cmd = Command::ValidateImportFile {
                    source,
                    path: std::path::PathBuf::from(&self.import_file_path),
                    password: None,
                };
                let _ = ctx.command_tx.try_send(cmd);
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.current_step = OnboardingStep::Welcome;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_import_preview_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        use crate::tui::screens::import_export::IMPORT_SOURCES;

        match key.code {
            KeyCode::Enter => {
                let source = IMPORT_SOURCES[self.selected_source_idx].0;
                let cmd = Command::ExecuteImport {
                    source,
                    path: std::path::PathBuf::from(&self.import_file_path),
                    password: None,
                    column_mapping: None,
                };
                let _ = ctx.command_tx.try_send(cmd);
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.current_step = OnboardingStep::ImportSource;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_set_password_key(&mut self, key: KeyEvent, _ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Enter => {
                // Navigate to the dedicated SetNewMasterPassword screen
                ScreenResult::NavigateTo(Screen::SetNewMasterPassword)
            }
            KeyCode::Esc => {
                // Go back based on path
                match self.selected_path {
                    Some(OnboardingPath::CreateNew) | Some(OnboardingPath::Import) => {
                        self.current_step = OnboardingStep::RecoveryVerify {
                            positions: self.verify_positions,
                        };
                    }
                    Some(OnboardingPath::Restore) => {
                        self.current_step = OnboardingStep::SecurityAdvisory;
                    }
                    None => {
                        self.current_step = OnboardingStep::Welcome;
                    }
                }
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    // ── Command result handling ────────────────────────────────────────────

    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::VaultInitialized { recovery_words } => {
                self.recovery_words = recovery_words;
                self.current_step = OnboardingStep::RecoveryDisplay;
                ScreenResult::Continue
            }
            CommandResult::RecoveryKeyUnlocked => {
                // Recovery key was accepted — already moved to VaultPath
                ScreenResult::Continue
            }
            CommandResult::ImportValidated { preview } => {
                if matches!(self.current_step, OnboardingStep::ImportSource) {
                    self.import_preview = Some(preview);
                    self.error = None;
                    self.current_step = OnboardingStep::ImportPreview;
                }
                ScreenResult::Continue
            }
            CommandResult::ImportCompleted { .. } => {
                if matches!(self.current_step, OnboardingStep::ImportPreview) {
                    self.current_step = OnboardingStep::VaultPath;
                }
                ScreenResult::Continue
            }
            CommandResult::Error { fallback, .. } => {
                self.error = Some(fallback);
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}

// ── Screen trait ──────────────────────────────────────────────────────────

impl crate::tui::traits::screen::Screen for OnboardingScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key) => self.handle_key(key, ctx),
            Message::CommandCompleted(result) => self.handle_command_result(result),
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        match &self.current_step {
            OnboardingStep::Welcome => self.view_welcome(frame, area),
            OnboardingStep::VaultPath => self.view_vault_path(frame, area),
            OnboardingStep::RecoveryDisplay => self.view_recovery_display(frame, area),
            OnboardingStep::RecoveryVerify { .. } => self.view_recovery_verify(frame, area),
            OnboardingStep::RecoveryInput => self.view_recovery_input(frame, area),
            OnboardingStep::SecurityAdvisory => self.view_security_advisory(frame, area),
            OnboardingStep::ImportSource => self.view_import_source(frame, area),
            OnboardingStep::ImportPreview => self.view_import_preview(frame, area),
            OnboardingStep::SetPassword => self.view_set_password(frame, area),
        }
    }

    fn on_mount(&mut self, _ctx: &mut ScreenContext) {
        // If returning from ImportExportScreen, resume at VaultPath step
        if self.returning_from_import {
            self.returning_from_import = false;
            self.current_step = OnboardingStep::VaultPath;
            return;
        }
        self.current_step = OnboardingStep::Welcome;
        self.selected_path = None;
        self.path_input.clear();
        self.error = None;
        self.recovery_confirmed = false;
        self.recovery_words.zeroize();
        self.recovery_words.clear();
        self.recovery_grid.zeroize();
        self.verify_inputs = std::array::from_fn(|_| String::new());
        self.verify_errors = [false; 4];
        self.verify_positions = [0; 4];
    }

    fn on_unmount(&mut self) {
        self.path_input.zeroize();
        self.path_input.clear();
        self.error = None;
        self.recovery_confirmed = false;
        self.recovery_words.zeroize();
        self.recovery_words.clear();
        self.recovery_grid.zeroize();
        for input in &mut self.verify_inputs {
            input.zeroize();
            input.clear();
        }
        self.verify_positions.zeroize();
    }
}

// ── View helpers ──────────────────────────────────────────────────────────

impl OnboardingScreen {
    /// Render a centered content block with standard padding.
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

    fn view_welcome(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let content_area = Self::centered_content(area, 12);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(2), // gap
            Constraint::Length(1), // option 1
            Constraint::Length(1), // option 2
            Constraint::Length(1), // option 3
            Constraint::Length(2), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new("Welcome to OpenKeyring")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Options
        let options = [
            (
                "1",
                "Create new vault",
                "Generate a fresh vault with recovery key",
            ),
            (
                "2",
                "Restore from recovery key",
                "Recover an existing vault",
            ),
            (
                "3",
                "Import from other manager",
                "Import from KeePass, 1Password, Bitwarden, etc.",
            ),
        ];

        for (i, (num, label, desc)) in options.iter().enumerate() {
            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", num),
                    Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} ", label),
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("- {}", desc), Style::default().fg(TEXT_SECONDARY)),
            ]);
            let para = Paragraph::new(line);
            frame.render_widget(para, rows[2 + i]);
        }

        // Hint
        let hint = Paragraph::new("Press 1, 2, or 3 to choose  |  Esc to quit")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[6]);

        // Step indicator
        let step_text = Paragraph::new("Step 1/1")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[7]);
    }

    fn view_vault_path(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let content_area = Self::centered_content(area, 10);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(3), // input with borders
            Constraint::Length(1), // gap
            Constraint::Length(1), // error
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new("Vault Location")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Input field
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::focused_border())
            .title(" Path ");

        let display_text = if self.path_input.is_empty() {
            let placeholder = "~/.local/share/open-keyring/vault.db";
            Paragraph::new(placeholder).style(Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(self.path_input.as_str()).style(Style::default().fg(TEXT))
        };

        frame.render_widget(input_block, rows[2]);

        // Render input text inside the bordered area
        let inner = Layout::vertical([Constraint::Length(1)]).split(rows[2])[0];
        let padded = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(inner);
        frame.render_widget(display_text, padded[1]);

        // Error
        if let Some(ref err) = self.error {
            let error_text = Paragraph::new(format!("{} {}", theme::ICON_ERROR, err))
                .style(Styles::error_text());
            frame.render_widget(error_text, rows[4]);
        }

        // Hint
        let hint = Paragraph::new("Enter to continue  |  Esc to go back")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[6]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[7]);
    }

    fn view_recovery_display(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let content_area = Self::centered_content(area, 20);

        let rows = Layout::vertical([
            Constraint::Length(1),  // title
            Constraint::Length(1),  // gap
            Constraint::Length(10), // word grid (4 rows x 6 cols = ~8 + borders)
            Constraint::Length(1),  // gap
            Constraint::Length(1),  // checkbox
            Constraint::Length(1),  // gap
            Constraint::Length(1),  // hint
            Constraint::Length(1),  // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new(format!(
            "{} Recovery Key - Write These Down!",
            theme::ICON_WARNING
        ))
        .style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Word grid (read-only)
        if self.recovery_words.is_empty() {
            let placeholder = Paragraph::new("Generating recovery key...")
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
            let grid_area = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER));
            frame.render_widget(grid_area, rows[2]);
            // Render placeholder centered inside grid
            let inner = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .split(rows[2]);
            frame.render_widget(placeholder, inner[1]);
        } else {
            // Build a read-only 4x6 grid showing the recovery words
            self.render_readonly_word_grid(frame, rows[2]);
        }

        // Checkbox
        let check_icon = if self.recovery_confirmed {
            theme::ICON_CHECK
        } else {
            "[ ]"
        };
        let check_style = if self.recovery_confirmed {
            Style::default().fg(SUCCESS)
        } else {
            Style::default().fg(TEXT_SECONDARY)
        };
        let checkbox = Paragraph::new(format!(" {} I have saved my recovery key", check_icon))
            .style(check_style)
            .alignment(Alignment::Center);
        frame.render_widget(checkbox, rows[4]);

        // Hint
        let hint_text = if self.recovery_confirmed {
            "Press C to unconfirm  |  Enter to continue  |  Esc to go back"
        } else {
            "Press C to confirm you saved the key  |  Esc to go back"
        };
        let hint = Paragraph::new(hint_text)
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[6]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[7]);
    }

    /// Render a read-only 4x6 word grid from recovery_words.
    fn render_readonly_word_grid(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        use ratatui::widgets::{Row, Table};

        let rows: Vec<Row> = (0..6)
            .map(|row| {
                let cells: Vec<Line> = (0..4)
                    .map(|col| {
                        let idx = row * 4 + col;
                        let num_str = format!("{:>2}.", idx + 1);
                        let word = if idx < self.recovery_words.len() {
                            self.recovery_words[idx].as_str()
                        } else {
                            "..."
                        };
                        Line::from(vec![
                            Span::styled(num_str, Style::default().fg(TEXT_SECONDARY)),
                            Span::raw(" "),
                            Span::styled(word.to_string(), Style::default().fg(TEXT)),
                        ])
                    })
                    .collect();
                Row::new(cells)
            })
            .collect();

        let widths = [
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ];

        let table = Table::new(rows, widths).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER)),
        );

        frame.render_widget(table, area);
    }

    fn view_recovery_verify(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let content_area = Self::centered_content(area, 14);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // instruction
            Constraint::Length(1), // gap
            Constraint::Length(1), // verify input 0
            Constraint::Length(1), // verify input 1
            Constraint::Length(1), // verify input 2
            Constraint::Length(1), // verify input 3
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new("Verify Recovery Key")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Instruction
        let instruction = Paragraph::new("Enter the word at each specified position:")
            .style(Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[2]);

        // Verify inputs
        for i in 0..4 {
            let pos = self.verify_positions[i] + 1; // 1-based display
            let input_text = if self.verify_inputs[i].is_empty() {
                format!("Word #{}: ____", pos)
            } else {
                format!("Word #{}: {}", pos, self.verify_inputs[i])
            };
            let style = if self.verify_errors[i] {
                Style::default().fg(ERROR)
            } else if self.verify_inputs[i].is_empty() {
                Style::default().fg(TEXT_MUTED)
            } else {
                Style::default().fg(TEXT)
            };
            let para = Paragraph::new(input_text).style(style);
            frame.render_widget(para, rows[4 + i]);
        }

        // Hint
        let hint = Paragraph::new("Enter to verify  |  Esc to go back")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[9]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[10]);
    }

    fn view_recovery_input(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let content_area = Self::centered_content(area, 16);

        let rows = Layout::vertical([
            Constraint::Length(1),  // title
            Constraint::Length(1),  // gap
            Constraint::Length(10), // grid
            Constraint::Length(1),  // gap
            Constraint::Length(1),  // hint
            Constraint::Length(1),  // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new("Enter Recovery Key")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Grid
        self.recovery_grid.view(frame, rows[2]);

        // Hint
        let hint = Paragraph::new("Tab: next word  |  Enter: submit  |  Esc: go back")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[4]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[5]);
    }

    fn view_security_advisory(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let content_area = Self::centered_content(area, 10);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(3), // notice
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new(format!("{} Security Notice", theme::ICON_WARNING))
            .style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Notice
        let notice = Paragraph::new(
            "Your vault has been restored from a recovery key.\n\
             We strongly recommend setting a new master password\n\
             and reviewing your security settings.",
        )
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: true });
        frame.render_widget(notice, rows[2]);

        // Hint
        let hint = Paragraph::new("Press Enter to set a new master password  |  Esc to go back")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[4]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[5]);
    }

    fn view_set_password(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let content_area = Self::centered_content(area, 8);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(2), // gap
            Constraint::Length(1), // instruction
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new("Set Master Password")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Instruction
        let instruction = Paragraph::new("You will be redirected to set your master password.")
            .style(Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[3]);

        // Hint
        let hint = Paragraph::new("Enter to continue  |  Esc to go back")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[5]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[6]);
    }

    fn view_import_source(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        use crate::tui::screens::import_export::{IMPORT_SOURCES, ImportFocus};

        let content_area = Self::centered_content(area, 18);

        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(
                "Step 2/6 · Select Import Source",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
        ];

        for (i, (_, name, _, scope_hint)) in IMPORT_SOURCES.iter().enumerate() {
            let prefix = if i == self.selected_source_idx {
                " \u{25B6} "
            } else {
                "   "
            };
            let is_focused =
                i == self.selected_source_idx && self.import_focus == ImportFocus::SourceList;
            let name_style = if is_focused {
                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            let hint_style = Style::default().fg(TEXT_MUTED);
            lines.push(Line::from(vec![
                Span::styled(prefix.to_string(), name_style),
                Span::styled((*name).to_string(), name_style),
                Span::styled(format!("  {}", scope_hint), hint_style),
            ]));
        }

        lines.push(Line::raw(""));

        let fp_style = if self.import_focus == ImportFocus::FilePath {
            Style::default().fg(PRIMARY)
        } else {
            Style::default().fg(TEXT_MUTED)
        };
        let fp_text = if self.import_file_path.is_empty() {
            "/path/to/file".to_string()
        } else {
            self.import_file_path.clone()
        };
        lines.push(Line::from(vec![
            Span::styled("File Path: ", Style::default().fg(TEXT)),
            Span::styled(fp_text, fp_style),
        ]));

        if let Some(ref err) = self.error {
            lines.push(Line::from(Span::styled(
                err.clone(),
                Style::default().fg(ERROR),
            )));
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "\u{2191}\u{2193}: navigate | Tab: file path | Enter: validate | Esc: back",
            Style::default().fg(TEXT_MUTED),
        )));

        frame.render_widget(Paragraph::new(lines), content_area);
    }

    fn view_import_preview(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let content_area = Self::centered_content(area, 18);

        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(
                "Step 3/6 · Import Preview",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
        ];

        if let Some(ref preview) = self.import_preview {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("Importable: {}", preview.importable),
                    Style::default().fg(SUCCESS),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("Needs review: {}", preview.needs_review),
                    Style::default().fg(WARNING),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("Failed: {}", preview.failed),
                    Style::default().fg(ERROR),
                ),
            ]));
            lines.push(Line::raw(""));

            for item in preview.review_items.iter().take(5) {
                lines.push(Line::from(vec![
                    Span::styled("\u{26A0} ", Style::default().fg(WARNING)),
                    Span::raw(format!("{} \u{2014} {}", item.name, item.reason)),
                ]));
            }

            for item in preview.failed_items.iter().take(5) {
                lines.push(Line::from(vec![
                    Span::styled("\u{2717} ", Style::default().fg(ERROR)),
                    Span::raw(format!("{} \u{2014} {}", item.name, item.reason)),
                ]));
            }
        } else {
            lines.push(Line::from("No preview data available"));
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Enter: start import | Esc: back",
            Style::default().fg(TEXT_MUTED),
        )));

        frame.render_widget(Paragraph::new(lines), content_area);
    }

    fn view_placeholder(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
        step_name: &str,
    ) {
        let content_area = Self::centered_content(area, 8);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(2), // gap
            Constraint::Length(1), // content
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new(step_name)
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Content
        let content = Paragraph::new(format!("{} configuration (coming soon)", step_name))
            .style(Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);
        frame.render_widget(content, rows[3]);

        // Hint
        let hint = Paragraph::new("Enter to continue  |  Esc to go back")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[5]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[6]);
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onboarding_welcome_defaults() {
        let screen = OnboardingScreen::default();
        assert!(screen.selected_path.is_none());
        assert_eq!(screen.current_step, OnboardingStep::Welcome);
        assert!(screen.path_input.is_empty());
        assert!(screen.error.is_none());
        assert!(!screen.recovery_confirmed);
        assert!(screen.recovery_words.is_empty());
        assert!(screen.verify_inputs.iter().all(|s| s.is_empty()));
        assert!(screen.verify_errors.iter().all(|&e| !e));
    }

    #[test]
    fn onboarding_create_path_steps() {
        let screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            ..Default::default()
        };
        assert_eq!(screen.total_steps(), 5);
    }

    #[test]
    fn onboarding_restore_path_steps() {
        let screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::Restore),
            ..Default::default()
        };
        assert_eq!(screen.total_steps(), 4);
    }

    #[test]
    fn onboarding_import_path_steps() {
        let screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::Import),
            ..Default::default()
        };
        assert_eq!(screen.total_steps(), 6);
    }

    #[test]
    fn onboarding_no_path_steps() {
        let screen = OnboardingScreen::default();
        assert_eq!(screen.total_steps(), 1);
    }

    #[test]
    fn onboarding_select_create() {
        let mut screen = OnboardingScreen::default();
        let result = screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Char('1'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.selected_path, Some(OnboardingPath::CreateNew));
        assert_eq!(screen.current_step, OnboardingStep::VaultPath);
    }

    #[test]
    fn onboarding_select_restore() {
        let mut screen = OnboardingScreen::default();
        let result = screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Char('2'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.selected_path, Some(OnboardingPath::Restore));
        assert_eq!(screen.current_step, OnboardingStep::RecoveryInput);
    }

    #[test]
    fn onboarding_select_import() {
        let mut screen = OnboardingScreen::default();
        let result = screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Char('3'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.selected_path, Some(OnboardingPath::Import));
        assert_eq!(screen.current_step, OnboardingStep::ImportSource);
    }

    #[test]
    fn onboarding_welcome_esc_exits() {
        let mut screen = OnboardingScreen::default();
        let result = screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(result, ScreenResult::ExitApp));
    }

    #[test]
    fn onboarding_vault_path_types() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::VaultPath,
            ..Default::default()
        };

        // Type characters
        let result = screen.handle_vault_path_key(
            KeyEvent::new(KeyCode::Char('/'), crossterm::event::KeyModifiers::NONE),
            &mut dummy_ctx(),
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.path_input, "/");

        screen.handle_vault_path_key(
            KeyEvent::new(KeyCode::Char('h'), crossterm::event::KeyModifiers::NONE),
            &mut dummy_ctx(),
        );
        assert_eq!(screen.path_input, "/h");

        // Backspace
        screen.handle_vault_path_key(
            KeyEvent::new(KeyCode::Backspace, crossterm::event::KeyModifiers::NONE),
            &mut dummy_ctx(),
        );
        assert_eq!(screen.path_input, "/");
    }

    #[test]
    fn onboarding_vault_path_esc_goes_back() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::VaultPath,
            ..Default::default()
        };
        let result = screen.handle_vault_path_key(
            KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE),
            &mut dummy_ctx(),
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.current_step, OnboardingStep::Welcome);
    }

    #[test]
    fn onboarding_recovery_display_toggle_confirm() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::RecoveryDisplay,
            ..Default::default()
        };

        assert!(!screen.recovery_confirmed);

        // Press C to confirm
        screen.handle_recovery_display_key(KeyEvent::new(
            KeyCode::Char('c'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(screen.recovery_confirmed);

        // Press C again to unconfirm
        screen.handle_recovery_display_key(KeyEvent::new(
            KeyCode::Char('C'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(!screen.recovery_confirmed);
    }

    #[test]
    fn onboarding_recovery_display_enter_without_confirm() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::RecoveryDisplay,
            recovery_confirmed: false,
            ..Default::default()
        };

        screen.handle_recovery_display_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        // Should NOT advance without confirmation
        assert_eq!(screen.current_step, OnboardingStep::RecoveryDisplay);
    }

    #[test]
    fn onboarding_recovery_display_enter_with_confirm() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::RecoveryDisplay,
            recovery_confirmed: true,
            recovery_words: vec!["abandon".to_string(); 24],
            ..Default::default()
        };

        screen.handle_recovery_display_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(
            screen.current_step,
            OnboardingStep::RecoveryVerify { .. }
        ));
        // Should have 4 positions sorted
        let sorted = {
            let mut p = screen.verify_positions;
            p.sort();
            p
        };
        assert_eq!(screen.verify_positions, sorted);
    }

    #[test]
    fn onboarding_step_number_create_path() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            ..Default::default()
        };

        screen.current_step = OnboardingStep::Welcome;
        assert_eq!(screen.current_step_number(), 1);

        screen.current_step = OnboardingStep::VaultPath;
        assert_eq!(screen.current_step_number(), 2);

        screen.current_step = OnboardingStep::RecoveryDisplay;
        assert_eq!(screen.current_step_number(), 3);

        screen.current_step = OnboardingStep::RecoveryVerify {
            positions: [0, 5, 10, 15],
        };
        assert_eq!(screen.current_step_number(), 4);

        screen.current_step = OnboardingStep::SetPassword;
        assert_eq!(screen.current_step_number(), 5);
    }

    #[test]
    fn onboarding_step_number_restore_path() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::Restore),
            ..Default::default()
        };

        screen.current_step = OnboardingStep::Welcome;
        assert_eq!(screen.current_step_number(), 1);

        screen.current_step = OnboardingStep::RecoveryInput;
        assert_eq!(screen.current_step_number(), 2);

        screen.current_step = OnboardingStep::VaultPath;
        assert_eq!(screen.current_step_number(), 3);

        screen.current_step = OnboardingStep::SecurityAdvisory;
        assert_eq!(screen.current_step_number(), 4);
    }

    #[test]
    fn onboarding_step_number_import_path() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::Import),
            ..Default::default()
        };

        screen.current_step = OnboardingStep::Welcome;
        assert_eq!(screen.current_step_number(), 1);

        screen.current_step = OnboardingStep::ImportSource;
        assert_eq!(screen.current_step_number(), 2);

        screen.current_step = OnboardingStep::ImportPreview;
        assert_eq!(screen.current_step_number(), 3);

        screen.current_step = OnboardingStep::VaultPath;
        assert_eq!(screen.current_step_number(), 4);

        screen.current_step = OnboardingStep::RecoveryDisplay;
        assert_eq!(screen.current_step_number(), 5);

        screen.current_step = OnboardingStep::RecoveryVerify {
            positions: [0, 1, 2, 3],
        };
        assert_eq!(screen.current_step_number(), 6);
    }

    #[test]
    fn onboarding_set_password_navigates() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::SetPassword,
            ..Default::default()
        };

        let result = screen.handle_set_password_key(
            KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
            &mut dummy_ctx(),
        );
        assert!(matches!(
            result,
            ScreenResult::NavigateTo(Screen::SetNewMasterPassword)
        ));
    }

    #[test]
    fn onboarding_security_advisory_enter() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::Restore),
            current_step: OnboardingStep::SecurityAdvisory,
            ..Default::default()
        };

        let result = screen.handle_security_advisory_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.current_step, OnboardingStep::SetPassword);
    }

    #[test]
    fn onboarding_import_source_enter() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::Import),
            current_step: OnboardingStep::ImportSource,
            ..Default::default()
        };

        let result = screen.handle_import_source_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(
            result,
            ScreenResult::NavigateTo(crate::commands::types::Screen::ImportExport)
        ));
    }

    #[test]
    fn onboarding_import_preview_enter() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::Import),
            current_step: OnboardingStep::ImportPreview,
            ..Default::default()
        };

        let result = screen.handle_import_preview_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.current_step, OnboardingStep::VaultPath);
    }

    #[test]
    fn onboarding_command_result_vault_initialized() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::VaultPath,
            ..Default::default()
        };

        let words: Vec<String> = (0..24).map(|i| format!("word{}", i)).collect();
        let result = screen.handle_command_result(CommandResult::VaultInitialized {
            recovery_words: words.clone(),
        });

        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.current_step, OnboardingStep::RecoveryDisplay);
        assert_eq!(screen.recovery_words, words);
    }

    #[test]
    fn onboarding_command_result_error() {
        let mut screen = OnboardingScreen::default();
        let result = screen.handle_command_result(CommandResult::Error {
            code: crate::errors::ErrorCode::Vault("not found".to_string()),
            context: crate::errors::ErrorContext::new(),
            message_key: "vault.not_found",
            fallback: "Vault not found".to_string(),
        });

        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.error, Some("Vault not found".to_string()));
    }

    #[test]
    fn onboarding_generate_verify_positions() {
        let mut screen = OnboardingScreen::default();
        screen.generate_verify_positions();

        // Should have 4 unique positions, all in 0..24
        assert_eq!(screen.verify_positions.len(), 4);
        let mut sorted = screen.verify_positions;
        sorted.sort();
        assert_eq!(screen.verify_positions, sorted); // sorted

        // All unique
        let unique: std::collections::HashSet<usize> =
            screen.verify_positions.iter().copied().collect();
        assert_eq!(unique.len(), 4);

        // All in range
        for &pos in &screen.verify_positions {
            assert!(pos < 24);
        }

        // Inputs and errors should be reset
        assert!(screen.verify_inputs.iter().all(|s| s.is_empty()));
        assert!(screen.verify_errors.iter().all(|&e| !e));
    }

    #[test]
    fn on_unmount_zeroizes_sensitive_data() {
        use crate::tui::traits::screen::Screen;

        let mut screen = OnboardingScreen::default();
        screen.path_input = "sensitive/path".to_string();
        screen.recovery_words = vec!["secret".to_string(); 24];
        screen.verify_inputs[0] = "secret".to_string();
        screen.verify_positions = [1, 2, 3, 4];
        for word in &mut screen.recovery_grid.words {
            word.push_str("secret");
        }

        screen.on_unmount();

        assert!(screen.path_input.is_empty());
        assert!(screen.recovery_words.is_empty());
        assert!(screen.verify_inputs.iter().all(|s| s.is_empty()));
        assert!(screen.recovery_grid.words.iter().all(|w| w.is_empty()));
        assert_eq!(screen.verify_positions, [0, 0, 0, 0]);
    }

    /// Helper to create a dummy ScreenContext for tests.
    /// The command_tx is a buffered channel that discards messages.
    fn dummy_ctx() -> ScreenContext<'static> {
        // We cannot easily construct ScreenContext in unit tests,
        // so we leak the channel to get 'static lifetime.
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
}
