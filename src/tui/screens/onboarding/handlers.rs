use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use crate::commands::result::CommandResult;
use crate::commands::types::Screen;
use crate::commands::Command;
use crate::t;
use crate::tui::traits::screen::{ScreenContext, ScreenResult};

use super::screen::OnboardingScreen;
use super::types::{OnboardingPath, OnboardingStep, RecoveryFocus};

fn contains(area: ratatui::layout::Rect, col: u16, row: u16) -> bool {
    area.left() <= col && col < area.right() && area.top() <= row && row < area.bottom()
}

impl OnboardingScreen {
    // ── Key handling ───────────────────────────────────────────────────────

    pub(crate) fn handle_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match &self.current_step {
            OnboardingStep::Welcome => self.handle_welcome_key(key, ctx),
            OnboardingStep::RecoveryDisplay => self.handle_recovery_display_key(key, ctx),
            OnboardingStep::RecoveryVerify { .. } => self.handle_recovery_verify_key(key),
            OnboardingStep::RecoveryInput => self.handle_recovery_input_key(key, ctx),
            OnboardingStep::SecurityAdvisory => self.handle_security_advisory_key(key),
            OnboardingStep::ImportSource => self.handle_import_source_key(key, ctx),
            OnboardingStep::ImportPreview => self.handle_import_preview_key(key, ctx),
            OnboardingStep::SetPassword => self.handle_set_password_key(key, ctx),
        }
    }

    // ── Mouse handling ───────────────────────────────────────────────────────

    pub(crate) fn handle_mouse(
        &mut self,
        event: MouseEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.handle_mouse_click(event.column, event.row, ctx)
            }
            MouseEventKind::Moved => self.handle_mouse_hover(event.column, event.row),
            _ => ScreenResult::Continue,
        }
    }

    fn handle_mouse_click(&mut self, col: u16, row: u16, ctx: &mut ScreenContext) -> ScreenResult {
        match self.current_step {
            OnboardingStep::Welcome => {
                for i in 0..self.welcome_card_areas.len() {
                    let area = self.welcome_card_areas[i].get();
                    if contains(area, col, row) {
                        self.welcome_selected = i;
                        return self.select_welcome_path();
                    }
                }
            }
            OnboardingStep::RecoveryDisplay => {
                return self.handle_recovery_display_click(col, row, ctx);
            }
            OnboardingStep::RecoveryVerify { .. } => {
                self.focus_verify_box_at(col, row);
            }
            _ => {}
        }
        ScreenResult::Continue
    }

    fn handle_mouse_hover(&mut self, col: u16, row: u16) -> ScreenResult {
        match self.current_step {
            OnboardingStep::Welcome => {
                for i in 0..self.welcome_card_areas.len() {
                    let area = self.welcome_card_areas[i].get();
                    if contains(area, col, row) {
                        if self.welcome_selected != i {
                            self.welcome_selected = i;
                        }
                        return ScreenResult::Continue;
                    }
                }
            }
            OnboardingStep::RecoveryDisplay => {
                let focus_targets = [
                    RecoveryFocus::CopyButton,
                    RecoveryFocus::RegenerateButton,
                    RecoveryFocus::LearnMoreToggle,
                    RecoveryFocus::ConfirmCheckbox,
                    RecoveryFocus::ConfirmCheckbox, // next step button shares checkbox area
                ];
                for (i, area_cell) in self.recovery_action_areas.iter().enumerate() {
                    let area = area_cell.get();
                    if contains(area, col, row) {
                        let new_focus = if i == 4 && !self.recovery_confirmed {
                            return ScreenResult::Continue;
                        } else {
                            focus_targets[i]
                        };
                        if self.recovery_focus != new_focus {
                            self.recovery_focus = new_focus;
                        }
                        return ScreenResult::Continue;
                    }
                }
            }
            OnboardingStep::RecoveryVerify { .. } => {
                self.focus_verify_box_at(col, row);
            }
            _ => {}
        }
        ScreenResult::Continue
    }

    fn focus_verify_box_at(&mut self, col: u16, row: u16) {
        for (i, area_cell) in self.verify_box_areas.iter().enumerate() {
            let area = area_cell.get();
            if contains(area, col, row) {
                self.verify_focus_index = i;
                return;
            }
        }
    }

    fn select_welcome_path(&mut self) -> ScreenResult {
        const LANGUAGES: [&str; 3] = ["auto", "en", "zh-CN"];
        let lang = LANGUAGES[self.language_index];
        match self.welcome_selected {
            0 => {
                self.selected_path = Some(OnboardingPath::CreateNew);
                self.generate_recovery_words(lang);
                self.set_step_forward(OnboardingStep::RecoveryDisplay);
            }
            1 => {
                self.selected_path = Some(OnboardingPath::Restore);
                self.pending_motion =
                    Some(crate::tui::state::animation::EffectKind::OnboardingForward);
                return ScreenResult::NavigateTo(Screen::KeyRecovery);
            }
            2 => {
                self.selected_path = Some(OnboardingPath::Import);
                self.set_step_forward(OnboardingStep::ImportSource);
            }
            _ => {}
        }
        ScreenResult::Continue
    }

    fn handle_recovery_display_click(
        &mut self,
        col: u16,
        row: u16,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        for (i, area_cell) in self.recovery_action_areas.iter().enumerate() {
            let area = area_cell.get();
            if !contains(area, col, row) {
                continue;
            }
            match i {
                0 => {
                    self.recovery_focus = RecoveryFocus::CopyButton;
                    if let Some(words) = &self.recovery_words {
                        let cmd = Command::CopyRawToClipboard {
                            value: words.to_phrase_secure(),
                        };
                        ctx.send_system_command(cmd);
                        self.clipboard_copied = true;
                        self.clipboard_clear_seconds = ctx.config.general.clipboard_clear_seconds;
                    }
                }
                1 => {
                    self.recovery_focus = RecoveryFocus::RegenerateButton;
                    const LANGUAGES: [&str; 3] = ["auto", "en", "zh-CN"];
                    let lang = LANGUAGES[self.language_index];
                    self.generate_recovery_words(lang);
                    self.recovery_confirmed = false;
                    self.clipboard_copied = false;
                }
                2 => self.learn_more_expanded = !self.learn_more_expanded,
                3 => self.recovery_confirmed = !self.recovery_confirmed,
                4 => {
                    if self.recovery_confirmed {
                        self.clipboard_copied = false;
                        self.generate_verify_positions();
                        self.set_step_forward(OnboardingStep::RecoveryVerify {
                            positions: self.verify_positions,
                        });
                    }
                }
                _ => {}
            }
            return ScreenResult::Continue;
        }
        ScreenResult::Continue
    }

    pub(crate) fn handle_welcome_key(
        &mut self,
        key: KeyEvent,
        _ctx: &mut ScreenContext,
    ) -> ScreenResult {
        const PATH_COUNT: usize = 3;
        const LANGUAGE_COUNT: usize = 3;
        const LANGUAGES: [&str; LANGUAGE_COUNT] = ["auto", "en", "zh-CN"];

        match key.code {
            KeyCode::Down | KeyCode::Tab => {
                self.welcome_selected = (self.welcome_selected + 1) % PATH_COUNT;
                ScreenResult::Continue
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.welcome_selected = (self.welcome_selected + PATH_COUNT - 1) % PATH_COUNT;
                ScreenResult::Continue
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                self.language_index = (self.language_index + 1) % LANGUAGE_COUNT;
                let lang = LANGUAGES[self.language_index];
                crate::tui::i18n::init(lang);
                ScreenResult::Continue
            }
            KeyCode::Enter => self.select_welcome_path(),
            KeyCode::Esc => ScreenResult::ExitApp,
            _ => ScreenResult::Continue,
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
                    RecoveryFocus::RegenerateButton => RecoveryFocus::LearnMoreToggle,
                    RecoveryFocus::LearnMoreToggle => RecoveryFocus::ConfirmCheckbox,
                    RecoveryFocus::ConfirmCheckbox => RecoveryFocus::CopyButton,
                };
                ScreenResult::Continue
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.recovery_focus = match self.recovery_focus {
                    RecoveryFocus::CopyButton => RecoveryFocus::ConfirmCheckbox,
                    RecoveryFocus::RegenerateButton => RecoveryFocus::CopyButton,
                    RecoveryFocus::LearnMoreToggle => RecoveryFocus::RegenerateButton,
                    RecoveryFocus::ConfirmCheckbox => RecoveryFocus::LearnMoreToggle,
                };
                ScreenResult::Continue
            }
            KeyCode::Enter => match self.recovery_focus {
                RecoveryFocus::CopyButton => {
                    if let Some(words) = &self.recovery_words {
                        let cmd = Command::CopyRawToClipboard {
                            value: words.to_phrase_secure(),
                        };
                        ctx.send_system_command(cmd);
                        self.clipboard_copied = true;
                        self.clipboard_clear_seconds = ctx.config.general.clipboard_clear_seconds;
                    }
                    ScreenResult::Continue
                }
                RecoveryFocus::RegenerateButton => {
                    const LANGUAGES: [&str; 3] = ["auto", "en", "zh-CN"];
                    let lang = LANGUAGES[self.language_index];
                    self.generate_recovery_words(lang);
                    self.recovery_confirmed = false;
                    self.clipboard_copied = false;
                    ScreenResult::Continue
                }
                RecoveryFocus::LearnMoreToggle => {
                    self.learn_more_expanded = !self.learn_more_expanded;
                    ScreenResult::Continue
                }
                RecoveryFocus::ConfirmCheckbox => {
                    if self.recovery_confirmed {
                        self.clipboard_copied = false;
                        self.generate_verify_positions();
                        self.set_step_forward(OnboardingStep::RecoveryVerify {
                            positions: self.verify_positions,
                        });
                    }
                    ScreenResult::Continue
                }
            },
            KeyCode::Char(' ') => {
                // Space toggles learn_more_expanded when LearnMoreToggle is focused
                if self.recovery_focus == RecoveryFocus::LearnMoreToggle {
                    self.learn_more_expanded = !self.learn_more_expanded;
                }
                // Space toggles checkbox when ConfirmCheckbox is focused
                if self.recovery_focus == RecoveryFocus::ConfirmCheckbox {
                    self.recovery_confirmed = !self.recovery_confirmed;
                }
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.clipboard_copied = false;
                self.set_step_back(OnboardingStep::Welcome);
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    pub(crate) fn handle_recovery_verify_key(&mut self, key: KeyEvent) -> ScreenResult {
        let focused = self.verify_focus_index;

        match key.code {
            KeyCode::Tab | KeyCode::Down => {
                self.verify_focus_index = (focused + 1) % self.verify_inputs.len();
                ScreenResult::Continue
            }
            KeyCode::Up => {
                self.verify_focus_index =
                    (focused + self.verify_inputs.len() - 1) % self.verify_inputs.len();
                ScreenResult::Continue
            }
            KeyCode::Enter => self.submit_recovery_verify(),
            KeyCode::Esc => {
                self.set_step_back(OnboardingStep::RecoveryDisplay);
                ScreenResult::Continue
            }
            KeyCode::Backspace => {
                self.verify_inputs[focused].pop_char();
                self.verify_errors[focused] = false;
                ScreenResult::Continue
            }
            KeyCode::Char(c) => {
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
            self.recovery_words
                .as_ref()
                .and_then(|words| words.word(pos))
                .is_some_and(|word| self.verify_inputs[i].expose(|s| s.eq_ignore_ascii_case(word)))
        });
        if all_correct {
            self.verify_errors = [false; 4];
            self.set_step_forward(OnboardingStep::SetPassword);
        } else {
            // Mark mismatches
            for (i, &pos) in self.verify_positions.iter().enumerate() {
                self.verify_errors[i] = !self
                    .recovery_words
                    .as_ref()
                    .and_then(|words| words.word(pos))
                    .is_some_and(|word| {
                        self.verify_inputs[i].expose(|s| s.eq_ignore_ascii_case(word))
                    });
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
                self.set_step_back(OnboardingStep::Welcome);
                ScreenResult::Continue
            }
            _ => {
                let result = self.recovery_grid.handle_key(key);
                match result {
                    Some(result) => {
                        match result {
                            Ok(words) => {
                                let cmd = Command::UnlockWithRecoveryKey { words };
                                if ctx.command_tx.try_send(cmd).is_err() {
                                    self.error =
                                        Some(t!("tui.error.command_dispatch_failed").to_string());
                                    return ScreenResult::Continue;
                                }
                                // Advance to SecurityAdvisory
                                self.set_step_forward(OnboardingStep::SecurityAdvisory);
                            }
                            Err(_) => {
                                self.error =
                                    Some(t!("tui.entry.key_recovery_empty_error").to_string());
                            }
                        }
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
                self.set_step_forward(OnboardingStep::SetPassword);
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.set_step_back(OnboardingStep::RecoveryInput);
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
            import_sources, source_needs_password, ImportFocus,
        };

        match key.code {
            KeyCode::Up => {
                if self.selected_source_idx > 0 {
                    self.selected_source_idx -= 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Down => {
                if self.selected_source_idx < import_sources().len() - 1 {
                    self.selected_source_idx += 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Tab => {
                let source = import_sources()[self.selected_source_idx].0;
                let needs_pw = source_needs_password(source);
                self.import_focus = match self.import_focus {
                    ImportFocus::SourceList => ImportFocus::FilePath,
                    ImportFocus::FilePath if needs_pw => ImportFocus::Password,
                    _ => ImportFocus::SourceList,
                };
                ScreenResult::Continue
            }
            KeyCode::BackTab => {
                let source = import_sources()[self.selected_source_idx].0;
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
                    self.error = Some(t!("tui.entry.file_path_required").to_string());
                    return ScreenResult::Continue;
                }
                let source = import_sources()[self.selected_source_idx].0;
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
                ctx.send_system_command(cmd);
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.set_step_back(OnboardingStep::Welcome);
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
        use crate::tui::screens::import_export::import_sources;

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
                let source = import_sources()[self.selected_source_idx].0;
                let cmd = Command::ExecuteImport {
                    session_id: self.import_session_id,
                    source,
                    path: std::path::PathBuf::from(&self.import_file_path),
                    password: None,
                    column_mapping: None,
                    import_as_notes: self.import_as_notes,
                };
                ctx.send_system_command(cmd);
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.set_step_back(OnboardingStep::ImportSource);
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
                        self.set_step_back(OnboardingStep::RecoveryVerify {
                            positions: self.verify_positions,
                        });
                    }
                    Some(OnboardingPath::Restore) => {
                        self.set_step_back(OnboardingStep::SecurityAdvisory);
                    }
                    None => {
                        self.set_step_back(OnboardingStep::Welcome);
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
                // Recovery key was accepted — advance to SecurityAdvisory
                self.set_step_forward(OnboardingStep::SecurityAdvisory);
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
                    self.set_step_forward(OnboardingStep::ImportPreview);
                }
                ScreenResult::Continue
            }
            CommandResult::ImportCompleted { .. } => {
                if matches!(self.current_step, OnboardingStep::ImportPreview) {
                    // After import, generate recovery words using the language selected on Welcome.
                    const LANGUAGES: [&str; 3] = ["auto", "en", "zh-CN"];
                    let lang = LANGUAGES[self.language_index];
                    self.generate_recovery_words(lang);
                    self.set_step_forward(OnboardingStep::RecoveryDisplay);
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
