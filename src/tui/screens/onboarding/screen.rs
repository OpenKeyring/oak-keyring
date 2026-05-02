use crossterm::event::{KeyCode, KeyEvent};
use zeroize::Zeroize;

use crate::commands::result::CommandResult;
use crate::commands::types::{ImportPreview, Screen};
use crate::commands::{Command, Message};
use crate::tui::screens::recovery_key::WordGridState;
use crate::tui::traits::screen::{ScreenContext, ScreenResult};
use crate::types::SecureStr;

use super::types::{OnboardingPath, OnboardingStep, RecoveryFocus};

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
    /// Currently focused verification input box index (0-3) on the RecoveryVerify step.
    pub verify_focus_index: usize,
    /// Signals that onboarding is returning from ImportExportScreen.
    /// When true, skip ImportSource step and go directly to VaultPath.
    pub returning_from_import: bool,
    // Import state for ImportSource/ImportPreview steps
    pub selected_source_idx: usize,
    pub import_file_path: String,
    pub import_password: String,
    pub import_focus: crate::tui::screens::import_export::ImportFocus,
    pub import_preview: Option<ImportPreview>,
    /// Whether to import problematic entries as notes instead of skipping them.
    pub import_as_notes: bool,
    /// Whether the checkbox on ImportPreview step is focused.
    pub import_preview_checkbox_focused: bool,
    // VaultPath step state
    /// Whether the path input is in editable (custom) mode.
    pub vault_path_editable: bool,
    /// Focus index for VaultPath step: 0=Use default button, 1=Custom button, 2=Path input (when editable).
    pub vault_path_focus: usize,
    // RecoveryDisplay step state
    /// Which element is focused on the RecoveryDisplay step.
    pub recovery_focus: RecoveryFocus,
    /// Whether recovery words have been copied to clipboard (show warning).
    pub clipboard_copied: bool,
    /// Clipboard clear timeout in seconds (captured from config when copying).
    pub clipboard_clear_seconds: u64,
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
            verify_focus_index: 0,
            returning_from_import: false,
            selected_source_idx: 0,
            import_file_path: String::new(),
            import_password: String::new(),
            import_focus: ImportFocus::SourceList,
            import_preview: None,
            import_as_notes: false,
            import_preview_checkbox_focused: false,
            vault_path_editable: false,
            vault_path_focus: 0,
            recovery_focus: RecoveryFocus::default(),
            clipboard_copied: false,
            clipboard_clear_seconds: 30,
        }
    }
}

impl OnboardingScreen {
    /// Generate 4 random positions for recovery verification.
    pub(crate) fn generate_verify_positions(&mut self) {
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
        self.verify_focus_index = 0;
    }

    /// Validate the current vault path and return a status message and severity.
    ///
    /// Returns `Some((message, is_error))` where `is_error` is true for blocking errors.
    pub(crate) fn validate_vault_path(&self) -> Option<(String, bool)> {
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
    pub(crate) fn resolved_vault_pathbuf(&self) -> std::path::PathBuf {
        if self.path_input.is_empty() {
            crate::config::general::default_vault_pathbuf()
        } else {
            std::path::PathBuf::from(&self.path_input)
        }
    }

    /// Total steps for the current path (including Welcome).
    pub(crate) fn total_steps(&self) -> usize {
        match self.selected_path {
            None => 1,
            Some(OnboardingPath::CreateNew) => 5, // Welcome + VaultPath + RecoveryDisplay + RecoveryVerify + SetPassword
            Some(OnboardingPath::Restore) => 4, // Welcome + RecoveryInput + VaultPath + SecurityAdvisory + SetPassword = 5... but spec says 3
            Some(OnboardingPath::Import) => 6, // Welcome + ImportSource + ImportPreview + VaultPath + RecoveryDisplay + RecoveryVerify + SetPassword
        }
    }

    /// Current step number (1-based).
    pub(crate) fn current_step_number(&self) -> usize {
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

    pub(crate) fn handle_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match &self.current_step {
            OnboardingStep::Welcome => self.handle_welcome_key(key),
            OnboardingStep::VaultPath => self.handle_vault_path_key(key, ctx),
            OnboardingStep::RecoveryDisplay => self.handle_recovery_display_key(key, ctx),
            OnboardingStep::RecoveryVerify { .. } => self.handle_recovery_verify_key(key),
            OnboardingStep::RecoveryInput => self.handle_recovery_input_key(key, ctx),
            OnboardingStep::SecurityAdvisory => self.handle_security_advisory_key(key),
            OnboardingStep::ImportSource => self.handle_import_source_key(key, ctx),
            OnboardingStep::ImportPreview => self.handle_import_preview_key(key, ctx),
            OnboardingStep::SetPassword => self.handle_set_password_key(key, ctx),
        }
    }

    pub(crate) fn handle_welcome_key(&mut self, key: KeyEvent) -> ScreenResult {
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

    pub(crate) fn handle_vault_path_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        if self.vault_path_editable {
            self.handle_vault_path_editable_key(key, ctx)
        } else {
            self.handle_vault_path_button_key(key, ctx)
        }
    }

    /// Handle key events when VaultPath is in non-editable (button) mode.
    pub(crate) fn handle_vault_path_button_key(
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
    pub(crate) fn handle_vault_path_editable_key(
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

    pub(crate) fn handle_recovery_display_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match key.code {
            KeyCode::Tab | KeyCode::Right => {
                self.recovery_focus = match self.recovery_focus {
                    RecoveryFocus::CopyButton => RecoveryFocus::RegenerateButton,
                    RecoveryFocus::RegenerateButton => RecoveryFocus::ConfirmCheckbox,
                    RecoveryFocus::ConfirmCheckbox => RecoveryFocus::CopyButton,
                };
                ScreenResult::Continue
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.recovery_focus = match self.recovery_focus {
                    RecoveryFocus::CopyButton => RecoveryFocus::ConfirmCheckbox,
                    RecoveryFocus::RegenerateButton => RecoveryFocus::CopyButton,
                    RecoveryFocus::ConfirmCheckbox => RecoveryFocus::RegenerateButton,
                };
                ScreenResult::Continue
            }
            KeyCode::Enter => match self.recovery_focus {
                RecoveryFocus::CopyButton => {
                    if !self.recovery_words.is_empty() {
                        let words_str = self.recovery_words.join(" ");
                        let cmd = Command::CopyRawToClipboard {
                            value: SecureStr::new(words_str),
                        };
                        let _ = ctx.command_tx.try_send(cmd);
                        self.clipboard_copied = true;
                        self.clipboard_clear_seconds = ctx.config.general.clipboard_clear_seconds;
                    }
                    ScreenResult::Continue
                }
                RecoveryFocus::RegenerateButton => {
                    // Re-initialize the vault with new recovery words so the
                    // displayed words match the actual vault's recovery key.
                    let vault_path = self.resolved_vault_pathbuf();
                    self.recovery_confirmed = false;
                    self.clipboard_copied = false;
                    let cmd = Command::InitializeVault {
                        vault_path,
                        master_password: SecureStr::new(String::new()),
                    };
                    let _ = ctx.command_tx.try_send(cmd);
                    ScreenResult::Continue
                }
                RecoveryFocus::ConfirmCheckbox => {
                    if self.recovery_confirmed {
                        self.generate_verify_positions();
                        self.current_step = OnboardingStep::RecoveryVerify {
                            positions: self.verify_positions,
                        };
                    }
                    ScreenResult::Continue
                }
            },
            KeyCode::Char(' ') => {
                // Space toggles checkbox when it is focused
                if self.recovery_focus == RecoveryFocus::ConfirmCheckbox {
                    self.recovery_confirmed = !self.recovery_confirmed;
                }
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.current_step = OnboardingStep::VaultPath;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    pub(crate) fn handle_recovery_verify_key(&mut self, key: KeyEvent) -> ScreenResult {
        let focused = self.verify_focus_index;

        match key.code {
            KeyCode::Tab => {
                if focused < 3 {
                    self.verify_focus_index = focused + 1;
                } else {
                    // On last box: submit if all filled
                    let all_filled = self.verify_inputs.iter().all(|s| !s.is_empty());
                    if all_filled {
                        return self.submit_recovery_verify();
                    }
                    // Otherwise clamp to last box
                }
                ScreenResult::Continue
            }
            KeyCode::BackTab => {
                if focused > 0 {
                    self.verify_focus_index = focused - 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Enter => self.submit_recovery_verify(),
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
            _ => ScreenResult::Continue,
        }
    }

    /// Validate the 4 verification inputs against the recovery words.
    /// On success, advance to SetPassword. On failure, mark errors.
    fn submit_recovery_verify(&mut self) -> ScreenResult {
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
                    || !self.verify_inputs[i].eq_ignore_ascii_case(&self.recovery_words[pos]);
            }
        }
        ScreenResult::Continue
    }

    pub(crate) fn handle_recovery_input_key(
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

    pub(crate) fn handle_security_advisory_key(&mut self, key: KeyEvent) -> ScreenResult {
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

    pub(crate) fn handle_import_source_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
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

    pub(crate) fn handle_import_preview_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        use crate::tui::screens::import_export::IMPORT_SOURCES;

        match key.code {
            KeyCode::Tab => {
                self.import_preview_checkbox_focused = !self.import_preview_checkbox_focused;
                ScreenResult::Continue
            }
            KeyCode::BackTab => {
                self.import_preview_checkbox_focused = !self.import_preview_checkbox_focused;
                ScreenResult::Continue
            }
            KeyCode::Char(' ') | KeyCode::Enter if self.import_preview_checkbox_focused => {
                self.import_as_notes = !self.import_as_notes;
                ScreenResult::Continue
            }
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
                    import_as_notes: self.import_as_notes,
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

    pub(crate) fn handle_set_password_key(
        &mut self,
        key: KeyEvent,
        _ctx: &mut ScreenContext,
    ) -> ScreenResult {
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

    pub(crate) fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
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
        self.verify_focus_index = 0;
        self.selected_source_idx = 0;
        self.import_file_path.clear();
        self.import_password.zeroize();
        self.import_password.clear();
        self.import_focus = crate::tui::screens::import_export::ImportFocus::SourceList;
        self.import_preview = None;
        self.import_as_notes = false;
        self.import_preview_checkbox_focused = false;
        self.vault_path_editable = false;
        self.vault_path_focus = 0;
        self.recovery_focus = RecoveryFocus::default();
        self.clipboard_copied = false;
        self.clipboard_clear_seconds = 30;
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
        self.verify_errors = [false; 4];
        self.verify_positions.zeroize();
        self.verify_focus_index = 0;
        self.import_file_path.zeroize();
        self.import_file_path.clear();
        self.import_password.zeroize();
        self.import_password.clear();
        self.import_preview = None;
        self.import_as_notes = false;
        self.import_preview_checkbox_focused = false;
        self.recovery_focus = RecoveryFocus::default();
        self.clipboard_copied = false;
        self.clipboard_clear_seconds = 30;
    }
}
