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
use crate::commands::types::{ImportPreview, Screen};
use crate::commands::{Command, Message};
use crate::tui::screens::recovery_key::WordGridState;
use crate::tui::theme::{
    self, Styles, BG, BG_SURFACE, BORDER, BRAND, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED,
    TEXT_PLACEHOLDER, TEXT_SECONDARY, WARNING,
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
    /// Currently highlighted card index on the Welcome step (0..3).
    pub welcome_selected: usize,
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
    pub import_password: String,
    pub import_focus: crate::tui::screens::import_export::ImportFocus,
    pub import_preview: Option<ImportPreview>,
    // VaultPath step state
    /// Whether the path input is in editable (custom) mode.
    pub vault_path_editable: bool,
    /// Focus index for VaultPath step: 0=Use default button, 1=Custom button, 2=Path input (when editable).
    pub vault_path_focus: usize,
}

/// Returns the default vault path as a display string for the VaultPath step.
fn default_vault_path_display() -> String {
    crate::config::general::default_vault_path_display()
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
            welcome_selected: 0,
            recovery_words: Vec::new(),
            recovery_grid: WordGridState::default(),
            verify_inputs: std::array::from_fn(|_| String::new()),
            verify_errors: [false; 4],
            verify_positions: [0; 4],
            returning_from_import: false,
            selected_source_idx: 0,
            import_file_path: String::new(),
            import_password: String::new(),
            import_focus: ImportFocus::SourceList,
            import_preview: None,
            vault_path_editable: false,
            vault_path_focus: 0,
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

    /// Validate the current vault path and return a status message and severity.
    ///
    /// Returns `Some((message, is_error))` where `is_error` is true for blocking errors.
    fn validate_vault_path(&self) -> Option<(String, bool)> {
        let path = self.resolved_vault_pathbuf();
        if path.as_os_str().is_empty() {
            return None;
        }

        if path.exists() {
            if !path.is_dir() {
                return Some(("Path exists but is not a directory".to_string(), true));
            }

            // Check write permission
            let write_target = path.join(".oak_write_test_tmp");
            let writable = std::fs::write(&write_target, b"").is_ok();
            if writable {
                let _ = std::fs::remove_file(&write_target);
            } else {
                return Some(("No write permission for this directory".to_string(), true));
            }

            // Check if directory is non-empty
            match std::fs::read_dir(path) {
                Ok(mut entries) => {
                    if entries.next().is_some() {
                        return Some((
                            "Directory is not empty — files may be overwritten".to_string(),
                            false,
                        ));
                    }
                }
                Err(e) => {
                    return Some((format!("Cannot read directory: {}", e), true));
                }
            }

            Some(("Path is valid".to_string(), false))
        } else {
            // Path does not exist — check if parent is writable
            match path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => {
                    if parent.exists() {
                        let write_target = parent.join(".oak_write_test_tmp");
                        let writable = std::fs::write(&write_target, b"").is_ok();
                        if writable {
                            let _ = std::fs::remove_file(&write_target);
                        }
                        if !writable {
                            return Some((
                                "No write permission for parent directory".to_string(),
                                true,
                            ));
                        }
                        Some(("Directory will be created automatically".to_string(), false))
                    } else {
                        // Parent also does not exist — check ancestor chain
                        match parent.parent() {
                            Some(grandparent) if !grandparent.as_os_str().is_empty() => {
                                let write_target = grandparent.join(".oak_write_test_tmp");
                                let writable = std::fs::write(&write_target, b"").is_ok();
                                if writable {
                                    let _ = std::fs::remove_file(&write_target);
                                }
                                if !writable {
                                    return Some((
                                        "Cannot create directory path".to_string(),
                                        true,
                                    ));
                                }
                                Some(("Directory will be created automatically".to_string(), false))
                            }
                            _ => Some(("Invalid path".to_string(), true)),
                        }
                    }
                }
                _ => Some(("Invalid path".to_string(), true)),
            }
        }
    }

    /// Resolve the actual vault path as a PathBuf for filesystem operations.
    fn resolved_vault_pathbuf(&self) -> std::path::PathBuf {
        if self.path_input.is_empty() {
            crate::config::general::default_vault_pathbuf()
        } else {
            std::path::PathBuf::from(&self.path_input)
        }
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
        const PATH_COUNT: usize = 3;

        match key.code {
            KeyCode::Down | KeyCode::Tab => {
                self.welcome_selected = (self.welcome_selected + 1) % PATH_COUNT;
                ScreenResult::Continue
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.welcome_selected = (self.welcome_selected + PATH_COUNT - 1) % PATH_COUNT;
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                match self.welcome_selected {
                    0 => {
                        self.selected_path = Some(OnboardingPath::CreateNew);
                        self.current_step = OnboardingStep::VaultPath;
                    }
                    1 => {
                        self.selected_path = Some(OnboardingPath::Restore);
                        self.current_step = OnboardingStep::RecoveryInput;
                    }
                    2 => {
                        self.selected_path = Some(OnboardingPath::Import);
                        self.current_step = OnboardingStep::ImportSource;
                    }
                    _ => {}
                }
                ScreenResult::Continue
            }
            KeyCode::Esc => ScreenResult::ExitApp,
            _ => ScreenResult::Continue,
        }
    }

    fn handle_vault_path_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        if self.vault_path_editable {
            self.handle_vault_path_editable_key(key, ctx)
        } else {
            self.handle_vault_path_button_key(key, ctx)
        }
    }

    /// Handle key events when VaultPath is in non-editable (button) mode.
    fn handle_vault_path_button_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match key.code {
            KeyCode::Tab | KeyCode::Right => {
                self.vault_path_focus = (self.vault_path_focus + 1) % 2;
                ScreenResult::Continue
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.vault_path_focus = (self.vault_path_focus + 1) % 2;
                ScreenResult::Continue
            }
            KeyCode::Enter => match self.vault_path_focus {
                0 => {
                    // "Use default path" — use the default and advance
                    self.path_input.clear();
                    self.advance_from_vault_path(ctx);
                    ScreenResult::Continue
                }
                1 => {
                    // "Custom path..." — switch to editable mode
                    self.vault_path_editable = true;
                    self.vault_path_focus = 2;
                    self.path_input.clear();
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            KeyCode::Esc => {
                self.current_step = OnboardingStep::Welcome;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    /// Handle key events when VaultPath is in editable (custom path input) mode.
    fn handle_vault_path_editable_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match key.code {
            KeyCode::Enter => {
                // Only advance if path validation passes (no blocking errors)
                let can_advance = self
                    .validate_vault_path()
                    .map(|(_, is_error)| !is_error)
                    .unwrap_or(false);
                if can_advance && !self.path_input.is_empty() {
                    self.advance_from_vault_path(ctx);
                }
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                // Return to non-editable button mode
                self.vault_path_editable = false;
                self.vault_path_focus = 1;
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
        // Resolve the actual path to use (default if input is empty)
        let vault_path = self.resolved_vault_pathbuf();

        match self.selected_path {
            Some(OnboardingPath::CreateNew) => {
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
        use crate::tui::screens::import_export::{
            source_needs_password, ImportFocus, IMPORT_SOURCES,
        };

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
                let source = IMPORT_SOURCES[self.selected_source_idx].0;
                let needs_pw = source_needs_password(source);
                self.import_focus = match self.import_focus {
                    ImportFocus::SourceList => ImportFocus::FilePath,
                    ImportFocus::FilePath if needs_pw => ImportFocus::Password,
                    _ => ImportFocus::SourceList,
                };
                ScreenResult::Continue
            }
            KeyCode::BackTab => {
                let source = IMPORT_SOURCES[self.selected_source_idx].0;
                let needs_pw = source_needs_password(source);
                self.import_focus = match self.import_focus {
                    ImportFocus::Password => ImportFocus::FilePath,
                    ImportFocus::FilePath => ImportFocus::SourceList,
                    _ if needs_pw => ImportFocus::Password,
                    _ => ImportFocus::FilePath,
                };
                ScreenResult::Continue
            }
            KeyCode::Char(c) => match self.import_focus {
                ImportFocus::FilePath => {
                    self.import_file_path.push(c);
                    ScreenResult::Continue
                }
                ImportFocus::Password => {
                    self.import_password.push(c);
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            KeyCode::Backspace => match self.import_focus {
                ImportFocus::FilePath => {
                    self.import_file_path.pop();
                    ScreenResult::Continue
                }
                ImportFocus::Password => {
                    self.import_password.pop();
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            KeyCode::Enter => {
                if self.import_file_path.is_empty() {
                    self.error = Some("File path is required".to_string());
                    return ScreenResult::Continue;
                }
                let source = IMPORT_SOURCES[self.selected_source_idx].0;
                let password = if self.import_password.is_empty() {
                    None
                } else {
                    Some(SecureStr::new(self.import_password.clone()))
                };
                let cmd = Command::ValidateImportFile {
                    source,
                    path: std::path::PathBuf::from(&self.import_file_path),
                    password,
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

    fn handle_import_preview_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        use crate::tui::screens::import_export::IMPORT_SOURCES;

        match key.code {
            KeyCode::Enter => {
                let source = IMPORT_SOURCES[self.selected_source_idx].0;
                let password = if self.import_password.is_empty() {
                    None
                } else {
                    Some(SecureStr::new(self.import_password.clone()))
                };
                let cmd = Command::ExecuteImport {
                    source,
                    path: std::path::PathBuf::from(&self.import_file_path),
                    password,
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
        self.welcome_selected = 0;
        self.recovery_words.zeroize();
        self.recovery_words.clear();
        self.recovery_grid.zeroize();
        self.verify_inputs = std::array::from_fn(|_| String::new());
        self.verify_errors = [false; 4];
        self.verify_positions = [0; 4];
        self.selected_source_idx = 0;
        self.import_file_path.clear();
        self.import_password.zeroize();
        self.import_password.clear();
        self.import_focus = crate::tui::screens::import_export::ImportFocus::SourceList;
        self.import_preview = None;
        self.vault_path_editable = false;
        self.vault_path_focus = 0;
    }

    fn on_unmount(&mut self) {
        self.path_input.zeroize();
        self.path_input.clear();
        self.error = None;
        self.recovery_confirmed = false;
        self.vault_path_editable = false;
        self.vault_path_focus = 0;
        self.recovery_words.zeroize();
        self.recovery_words.clear();
        self.recovery_grid.zeroize();
        for input in &mut self.verify_inputs {
            input.zeroize();
            input.clear();
        }
        self.verify_positions.zeroize();
        self.import_file_path.zeroize();
        self.import_file_path.clear();
        self.import_password.zeroize();
        self.import_password.clear();
        self.import_preview = None;
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
        let content_area = Self::centered_content(area, 20);

        let rows = Layout::vertical([
            Constraint::Length(1), // brand
            Constraint::Length(1), // separator line
            Constraint::Length(1), // subtitle
            Constraint::Length(1), // gap
            Constraint::Length(3), // card 0 — CreateNew
            Constraint::Length(1), // gap
            Constraint::Length(3), // card 1 — Restore
            Constraint::Length(1), // gap
            Constraint::Length(3), // card 2 — Import
            Constraint::Length(2), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
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
        let separator = Paragraph::new(Span::styled(
            "\u{2500}".repeat(40),
            Style::default().fg(BORDER),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(separator, rows[1]);

        // Subtitle
        let subtitle = Paragraph::new(Span::styled(
            "Secure, open-source terminal password manager",
            Style::default().fg(TEXT_SECONDARY),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(subtitle, rows[2]);

        // Cards
        let cards = [
            (
                "\u{2726}", // ✦
                "Create new vault",
                "Start fresh \u{2014} generate recovery key and set password",
            ),
            (
                "\u{21BB}", // ↻
                "Restore existing vault",
                "Recover an OpenKeyring vault using a recovery key",
            ),
            (
                "\u{2193}", // ↓
                "Import from other manager",
                "Migrate from KeePass, 1Password, Bitwarden, etc.",
            ),
        ];

        for (i, (icon, title, desc)) in cards.iter().enumerate() {
            let is_selected = i == self.welcome_selected;
            let card_row = rows[4 + i * 2];

            let border_color = if is_selected { PRIMARY } else { BORDER };
            let bg_color = if is_selected { BG_SURFACE } else { BG };

            let card_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(bg_color));

            let inner = card_block.inner(card_row);
            frame.render_widget(card_block, card_row);

            // Two lines inside the card: icon + title, then description
            let card_lines = Layout::vertical([
                Constraint::Length(1), // icon + title
                Constraint::Length(1), // description
            ])
            .split(inner);

            let title_line = Paragraph::new(Line::from(vec![
                Span::styled(format!(" {} ", icon), Style::default().fg(BRAND)),
                Span::styled(
                    title.to_string(),
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
            ]));
            frame.render_widget(title_line, card_lines[0]);

            let desc_line = Paragraph::new(Line::from(Span::styled(
                format!("   {}", desc),
                Style::default().fg(TEXT_SECONDARY),
            )));
            frame.render_widget(desc_line, card_lines[1]);
        }

        // Hint
        let hint = Paragraph::new("\u{2191}\u{2193}/Tab: navigate  |  Enter: select  |  Esc: quit")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[10]);

        // Step indicator
        let step_text = Paragraph::new("Step 1/1")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[11]);
    }

    fn view_vault_path(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let content_area = Self::centered_content(area, 14);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // description
            Constraint::Length(1), // gap
            Constraint::Length(3), // path display / input with borders
            Constraint::Length(1), // gap
            Constraint::Length(1), // validation status
            Constraint::Length(1), // gap
            Constraint::Length(1), // buttons (non-editable) or hint (editable)
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new("Vault Storage")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Description
        let desc = Paragraph::new("Choose where to store the encrypted database and config files.")
            .style(Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);
        frame.render_widget(desc, rows[2]);

        // Path display / input field
        let (border_style, display_text) = if self.vault_path_editable {
            // Editable mode — show actual input with cursor
            let input_style = if self.path_input.is_empty() {
                Style::default().fg(TEXT_PLACEHOLDER)
            } else {
                Style::default().fg(TEXT)
            };
            let text = if self.path_input.is_empty() {
                "Enter custom path...".to_string()
            } else {
                format!("{}_", self.path_input)
            };
            (
                Styles::focused_border(),
                Paragraph::new(text).style(input_style),
            )
        } else {
            // Read-only mode — show default or chosen path
            (
                Style::default().fg(BORDER),
                Paragraph::new(default_vault_path_display())
                    .style(Style::default().fg(TEXT_SECONDARY)),
            )
        };

        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Vault Path ");

        frame.render_widget(input_block, rows[4]);

        // Render text inside the bordered area
        let inner = Layout::vertical([Constraint::Length(1)]).split(rows[4])[0];
        let padded = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(inner);
        frame.render_widget(display_text, padded[1]);

        // Validation status (show when no command error is present)
        if self.error.is_none() {
            if let Some((msg, is_error)) = self.validate_vault_path() {
                let icon = if is_error {
                    theme::ICON_ERROR
                } else {
                    match msg.as_str() {
                        "Path is valid" => theme::ICON_SUCCESS,
                        _ => theme::ICON_WARNING,
                    }
                };
                let style = if is_error {
                    Styles::error_text()
                } else {
                    match msg.as_str() {
                        "Path is valid" => Styles::success_text(),
                        _ => Styles::warning_text(),
                    }
                };
                let status = Paragraph::new(format!("{} {}", icon, msg)).style(style);
                frame.render_widget(status, rows[6]);
            }
        }

        // Error from command result (takes precedence)
        if let Some(ref err) = self.error {
            let error_text = Paragraph::new(format!("{} {}", theme::ICON_ERROR, err))
                .style(Styles::error_text());
            frame.render_widget(error_text, rows[6]);
        }

        // Buttons or mode hint
        if self.vault_path_editable {
            let mode_hint = Paragraph::new("Enter: confirm  |  Esc: cancel custom path")
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
            frame.render_widget(mode_hint, rows[8]);
        } else {
            // Two side-by-side buttons
            let btn_area = Layout::horizontal([
                Constraint::Fill(1),
                Constraint::Length(22),
                Constraint::Length(2),
                Constraint::Length(20),
                Constraint::Fill(1),
            ])
            .split(rows[8]);

            let default_btn_style = if self.vault_path_focus == 0 {
                Styles::button_primary()
            } else {
                Styles::button_secondary()
            };
            let default_btn = Paragraph::new(" Use default path ")
                .style(default_btn_style)
                .alignment(Alignment::Center);
            frame.render_widget(default_btn, btn_area[1]);

            let custom_btn_style = if self.vault_path_focus == 1 {
                Styles::button_primary()
            } else {
                Styles::button_secondary()
            };
            let custom_btn = Paragraph::new(" Custom path... ")
                .style(custom_btn_style)
                .alignment(Alignment::Center);
            frame.render_widget(custom_btn, btn_area[3]);
        }

        // Hint
        let hint = if self.vault_path_editable {
            "Type a path  |  Enter: confirm  |  Esc: cancel"
        } else {
            "\u{2190}\u{2192}/Tab: switch  |  Enter: select  |  Esc: back"
        };
        let hint = Paragraph::new(hint)
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[10]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[11]);
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
        use crate::tui::screens::import_export::{
            source_needs_password, ImportFocus, IMPORT_SOURCES,
        };

        let content_area = Self::centered_content(area, 20);
        let source = IMPORT_SOURCES[self.selected_source_idx].0;
        let needs_pw = source_needs_password(source);

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

        if needs_pw {
            let pw_style = if self.import_focus == ImportFocus::Password {
                Style::default().fg(PRIMARY)
            } else {
                Style::default().fg(TEXT_MUTED)
            };
            let pw_display = if self.import_password.is_empty() {
                "password".to_string()
            } else {
                "*".repeat(self.import_password.len())
            };
            lines.push(Line::from(vec![
                Span::styled("Password: ", Style::default().fg(TEXT)),
                Span::styled(pw_display, pw_style),
            ]));
        }

        if let Some(ref err) = self.error {
            lines.push(Line::from(Span::styled(
                err.clone(),
                Style::default().fg(ERROR),
            )));
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "\u{2191}\u{2193}: navigate | Tab: cycle fields | Enter: validate | Esc: back",
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
        assert_eq!(screen.welcome_selected, 0);
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
    fn onboarding_welcome_default_selected_is_first() {
        let screen = OnboardingScreen::default();
        assert_eq!(screen.welcome_selected, 0);
    }

    #[test]
    fn onboarding_welcome_enter_selects_create() {
        let mut screen = OnboardingScreen::default();
        // Default selection is 0 (CreateNew), pressing Enter should select it
        let result = screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.selected_path, Some(OnboardingPath::CreateNew));
        assert_eq!(screen.current_step, OnboardingStep::VaultPath);
    }

    #[test]
    fn onboarding_welcome_down_then_enter_selects_restore() {
        let mut screen = OnboardingScreen::default();
        // Press Down to move to index 1 (Restore)
        screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(screen.welcome_selected, 1);

        let result = screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.selected_path, Some(OnboardingPath::Restore));
        assert_eq!(screen.current_step, OnboardingStep::RecoveryInput);
    }

    #[test]
    fn onboarding_welcome_down_twice_then_enter_selects_import() {
        let mut screen = OnboardingScreen::default();
        screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(screen.welcome_selected, 2);

        let result = screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.selected_path, Some(OnboardingPath::Import));
        assert_eq!(screen.current_step, OnboardingStep::ImportSource);
    }

    #[test]
    fn onboarding_welcome_down_wraps_around() {
        let mut screen = OnboardingScreen::default();
        // Down three times from 0 should wrap back to 0
        screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(screen.welcome_selected, 1);
        screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(screen.welcome_selected, 2);
        screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(screen.welcome_selected, 0);
    }

    #[test]
    fn onboarding_welcome_up_wraps_around() {
        let mut screen = OnboardingScreen::default();
        // Up from 0 should wrap to 2
        screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Up,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(screen.welcome_selected, 2);
    }

    #[test]
    fn onboarding_welcome_tab_moves_down() {
        let mut screen = OnboardingScreen::default();
        screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(screen.welcome_selected, 1);
    }

    #[test]
    fn onboarding_welcome_backtab_moves_up() {
        let mut screen = OnboardingScreen::default();
        screen.welcome_selected = 2;
        screen.handle_welcome_key(KeyEvent::new(
            KeyCode::BackTab,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(screen.welcome_selected, 1);
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
    fn onboarding_welcome_ignores_number_keys() {
        let mut screen = OnboardingScreen::default();
        // Number keys no longer select paths — only navigation + Enter
        let result = screen.handle_welcome_key(KeyEvent::new(
            KeyCode::Char('1'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(result, ScreenResult::Continue));
        // Should not have changed state
        assert_eq!(screen.welcome_selected, 0);
        assert!(screen.selected_path.is_none());
    }

    #[test]
    fn onboarding_vault_path_defaults_to_non_editable() {
        let screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::VaultPath,
            ..Default::default()
        };
        assert!(!screen.vault_path_editable);
        assert_eq!(screen.vault_path_focus, 0);
    }

    #[test]
    fn onboarding_vault_path_non_editable_ignores_chars() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::VaultPath,
            ..Default::default()
        };
        // In non-editable mode, typing characters should be ignored
        let result = screen.handle_vault_path_key(
            KeyEvent::new(KeyCode::Char('/'), crossterm::event::KeyModifiers::NONE),
            &mut dummy_ctx(),
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.path_input.is_empty());
    }

    #[test]
    fn onboarding_vault_path_tab_cycles_buttons() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::VaultPath,
            ..Default::default()
        };
        assert_eq!(screen.vault_path_focus, 0);
        screen.handle_vault_path_key(
            KeyEvent::new(KeyCode::Tab, crossterm::event::KeyModifiers::NONE),
            &mut dummy_ctx(),
        );
        assert_eq!(screen.vault_path_focus, 1);
        screen.handle_vault_path_key(
            KeyEvent::new(KeyCode::Tab, crossterm::event::KeyModifiers::NONE),
            &mut dummy_ctx(),
        );
        assert_eq!(screen.vault_path_focus, 0);
    }

    #[test]
    fn onboarding_vault_path_enter_default_uses_default() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::VaultPath,
            vault_path_focus: 0,
            ..Default::default()
        };
        let result = screen.handle_vault_path_key(
            KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
            &mut dummy_ctx(),
        );
        assert!(matches!(result, ScreenResult::Continue));
        // Path input stays empty (default path resolved at advance time)
        assert!(screen.path_input.is_empty());
    }

    #[test]
    fn onboarding_vault_path_enter_custom_switches_to_editable() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::VaultPath,
            vault_path_focus: 1,
            ..Default::default()
        };
        let result = screen.handle_vault_path_key(
            KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
            &mut dummy_ctx(),
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.vault_path_editable);
        assert_eq!(screen.vault_path_focus, 2);
    }

    #[test]
    fn onboarding_vault_path_editable_types() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::VaultPath,
            vault_path_editable: true,
            vault_path_focus: 2,
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
    fn onboarding_vault_path_esc_returns_to_button_mode() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::CreateNew),
            current_step: OnboardingStep::VaultPath,
            vault_path_editable: true,
            vault_path_focus: 2,
            ..Default::default()
        };
        let result = screen.handle_vault_path_key(
            KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE),
            &mut dummy_ctx(),
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert!(!screen.vault_path_editable);
        assert_eq!(screen.vault_path_focus, 1);
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
        let mut ctx = dummy_ctx();

        let result = screen.handle_import_source_key(
            KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
            &mut ctx,
        );
        // Import source Enter now validates the file (requires file path),
        // so without a file path it shows an error instead of navigating.
        assert!(matches!(result, ScreenResult::Continue));
    }

    #[test]
    fn onboarding_import_preview_enter() {
        let mut screen = OnboardingScreen {
            selected_path: Some(OnboardingPath::Import),
            current_step: OnboardingStep::ImportPreview,
            ..Default::default()
        };
        let mut ctx = dummy_ctx();

        let result = screen.handle_import_preview_key(
            KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        // ImportPreview Enter sends ExecuteImport command, stays on ImportPreview
        // until ImportCompleted result is received.
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

    #[test]
    fn onboarding_vault_path_validate_default_path() {
        let screen = OnboardingScreen {
            path_input: String::new(),
            ..Default::default()
        };
        // When path_input is empty, resolved_vault_pathbuf returns the actual default path
        let result = screen.validate_vault_path();
        assert!(result.is_some());
        let (_msg, is_error) = result.unwrap();
        // Default path exists and is writable, so it should not be a blocking error
        assert!(!is_error, "Default path should not have a blocking error");
    }

    #[test]
    fn onboarding_vault_path_validate_nonexistent_directory() {
        let screen = OnboardingScreen {
            path_input: "/tmp/oak_test_nonexistent_dir_12345".to_string(),
            ..Default::default()
        };
        let result = screen.validate_vault_path();
        assert!(result.is_some());
        let (msg, is_error) = result.unwrap();
        assert!(
            !is_error,
            "Non-existent directory should be a warning, got: {}",
            msg
        );
        assert!(
            msg.to_lowercase().contains("creat"),
            "Message should mention creation, got: {}",
            msg
        );
    }

    #[test]
    fn onboarding_vault_path_validate_existing_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let screen = OnboardingScreen {
            path_input: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        let result = screen.validate_vault_path();
        assert!(result.is_some());
        let (msg, is_error) = result.unwrap();
        assert!(
            !is_error,
            "Existing empty dir should be valid, got: {}",
            msg
        );
        assert!(
            msg.contains("valid"),
            "Message should say valid, got: {}",
            msg
        );
    }

    #[test]
    fn onboarding_vault_path_validate_existing_nonempty_dir() {
        let dir = tempfile::tempdir().unwrap();
        // Create a file to make the directory non-empty
        std::fs::write(dir.path().join("test.txt"), b"hello").unwrap();
        let screen = OnboardingScreen {
            path_input: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        let result = screen.validate_vault_path();
        assert!(result.is_some());
        let (msg, is_error) = result.unwrap();
        assert!(
            !is_error,
            "Non-empty dir should be a warning, not error, got: {}",
            msg
        );
        assert!(
            msg.to_lowercase().contains("not empty"),
            "Message should mention not empty, got: {}",
            msg
        );
    }

    #[test]
    fn onboarding_vault_path_validate_file_not_directory() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let screen = OnboardingScreen {
            path_input: file.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        let result = screen.validate_vault_path();
        assert!(result.is_some());
        let (msg, is_error) = result.unwrap();
        assert!(is_error, "File path should be an error, got: {}", msg);
    }

    #[test]
    fn onboarding_vault_path_resolved_default_when_empty() {
        let screen = OnboardingScreen {
            path_input: String::new(),
            ..Default::default()
        };
        let resolved = screen.resolved_vault_pathbuf();
        let resolved_str = resolved.to_string_lossy();
        assert!(!resolved_str.is_empty());
        assert!(
            resolved_str.contains("open-keyring"),
            "Default path should contain 'open-keyring', got: {}",
            resolved_str
        );
    }

    #[test]
    fn onboarding_vault_path_resolved_custom_when_set() {
        let screen = OnboardingScreen {
            path_input: "/custom/path".to_string(),
            ..Default::default()
        };
        let resolved = screen.resolved_vault_pathbuf();
        assert_eq!(resolved, std::path::PathBuf::from("/custom/path"));
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
