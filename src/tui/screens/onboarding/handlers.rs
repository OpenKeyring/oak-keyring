use crossterm::event::{KeyCode, KeyEvent};

use crate::commands::result::CommandResult;
use crate::commands::types::Screen;
use crate::commands::Command;
use crate::tui::traits::screen::{ScreenContext, ScreenResult};
use crate::types::sensitive::SecureStr;

use super::screen::OnboardingScreen;
use super::types::{OnboardingPath, OnboardingStep, RecoveryFocus};

impl OnboardingScreen {
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
        match self.selected_path {
            Some(OnboardingPath::CreateNew) | Some(OnboardingPath::Import) => {
                self.generate_recovery_words(&ctx.config.general.language);
                self.current_step = OnboardingStep::RecoveryDisplay;
            }
            Some(OnboardingPath::Restore) => {
                self.current_step = OnboardingStep::SecurityAdvisory;
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
                    self.generate_recovery_words(&ctx.config.general.language);
                    self.recovery_confirmed = false;
                    self.clipboard_copied = false;
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
                self.verify_inputs[focused].pop_char();
                self.verify_errors[focused] = false;
                ScreenResult::Continue
            }
            KeyCode::Char(c) if c.is_alphabetic() => {
                let input = &mut self.verify_inputs[focused];
                if input.len() < 12 {
                    input.push_char(c);
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
                && self.verify_inputs[i]
                    .expose(|s| s.eq_ignore_ascii_case(&self.recovery_words[pos]))
        });
        if all_correct {
            self.verify_errors = [false; 4];
            self.current_step = OnboardingStep::SetPassword;
        } else {
            // Mark mismatches
            for (i, &pos) in self.verify_positions.iter().enumerate() {
                self.verify_errors[i] = pos >= self.recovery_words.len()
                    || !self.verify_inputs[i]
                        .expose(|s| s.eq_ignore_ascii_case(&self.recovery_words[pos]));
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
                    self.import_password.push_char(c);
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
                    self.import_password.pop_char();
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
                    Some(self.import_password.take_secure())
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
                let cmd = Command::ExecuteImport {
                    session_id: self.import_session_id,
                    source,
                    path: std::path::PathBuf::from(&self.import_file_path),
                    password: None,
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
            CommandResult::RecoveryKeyUnlocked => {
                // Recovery key was accepted — already moved to VaultPath
                ScreenResult::Continue
            }
            CommandResult::ImportValidated {
                session_id,
                preview,
            } => {
                if matches!(self.current_step, OnboardingStep::ImportSource) {
                    self.import_session_id = Some(session_id);
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
