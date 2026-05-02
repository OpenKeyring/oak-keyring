use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use zeroize::Zeroize;

use crate::commands::result::CommandResult;
use crate::commands::types::{CsvColumnMapping, ExportScope, ImportPreview, ImportSource};
use crate::commands::{Command, Message};
use crate::crypto::strength::{evaluate_strength, PasswordStrength, StrengthLevel};
use crate::tui::theme::{ERROR, PRIMARY, SUCCESS, WARNING};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::SecureStr;

use super::types::*;

// ── ImportExportScreen ──────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ImportExportScreen {
    pub mode: ImportExportMode,
    pub entry_point: ImportEntryPoint,

    // Import state
    pub import_step: ImportStep,
    pub selected_source_idx: usize,
    pub source: Option<ImportSource>,
    pub file_path: String,
    pub decrypt_password: String,
    pub import_focus: ImportFocus,
    pub csv_mapping: CsvColumnMapping,
    pub preview: Option<ImportPreview>,
    pub import_progress_current: usize,
    pub import_progress_total: usize,
    pub import_progress_name: String,
    pub imported_count: usize,
    pub skipped_count: usize,

    // Export state
    pub export_step: ExportStep,
    pub export_focus: ExportFocus,
    pub export_scope_option: ExportScopeOption,
    pub export_password: String,
    pub export_confirm_password: String,
    pub export_password_strength: Option<PasswordStrength>,
    pub export_output_path: String,
    pub master_password: String,
    pub export_result_path: Option<PathBuf>,
    pub export_record_count: usize,

    // Shared
    pub error_message: Option<String>,
}

impl ImportExportScreen {
    pub fn new() -> Self {
        Self {
            mode: ImportExportMode::Import,
            entry_point: ImportEntryPoint::ConfigPage,

            import_step: ImportStep::SourceSelect,
            selected_source_idx: 0,
            source: None,
            file_path: String::new(),
            decrypt_password: String::new(),
            import_focus: ImportFocus::SourceList,
            csv_mapping: CsvColumnMapping {
                name_column: "Title".to_string(),
                username_column: "Username".to_string(),
                password_column: "Password".to_string(),
                url_column: "URL".to_string(),
                notes_column: "Notes".to_string(),
                tags_column: None,
                skip_header: true,
            },
            preview: None,
            import_progress_current: 0,
            import_progress_total: 0,
            import_progress_name: String::new(),
            imported_count: 0,
            skipped_count: 0,

            export_step: ExportStep::Form,
            export_focus: ExportFocus::Scope,
            export_scope_option: ExportScopeOption::All,
            export_password: String::new(),
            export_confirm_password: String::new(),
            export_password_strength: None,
            export_output_path: default_export_path(),
            master_password: String::new(),
            export_result_path: None,
            export_record_count: 0,

            error_message: None,
        }
    }

    /// Navigate back to the appropriate screen based on entry point.
    pub(super) fn go_back(&self) -> ScreenResult {
        match self.entry_point {
            ImportEntryPoint::Onboarding { .. } | ImportEntryPoint::ConfigPage => {
                ScreenResult::PopScreen
            }
        }
    }

    pub(super) fn display_password(password: &str) -> String {
        "\u{2022}".repeat(password.len())
    }

    pub(super) fn strength_color(level: &StrengthLevel) -> ratatui::style::Color {
        match level {
            StrengthLevel::VeryWeak | StrengthLevel::Weak => ERROR,
            StrengthLevel::Fair => WARNING,
            StrengthLevel::Strong => PRIMARY,
            StrengthLevel::VeryStrong => SUCCESS,
        }
    }

    pub(super) fn update_export_strength(&mut self) {
        if self.export_password.is_empty() {
            self.export_password_strength = None;
        } else {
            self.export_password_strength = Some(evaluate_strength(&self.export_password));
        }
    }

    pub(super) fn current_source(&self) -> ImportSource {
        IMPORT_SOURCES[self.selected_source_idx].0
    }

    pub(super) fn import_focus_cycle_next(&mut self) {
        let has_password = source_needs_password(self.current_source());
        let is_csv = self.current_source() == ImportSource::Csv;

        self.import_focus = match self.import_focus {
            ImportFocus::SourceList => ImportFocus::FilePath,
            ImportFocus::FilePath => {
                if has_password {
                    ImportFocus::Password
                } else if is_csv {
                    ImportFocus::CsvName
                } else {
                    ImportFocus::SourceList
                }
            }
            ImportFocus::Password => {
                if is_csv {
                    ImportFocus::CsvName
                } else {
                    ImportFocus::SourceList
                }
            }
            ImportFocus::CsvName => ImportFocus::CsvUsername,
            ImportFocus::CsvUsername => ImportFocus::CsvPassword,
            ImportFocus::CsvPassword => ImportFocus::CsvUrl,
            ImportFocus::CsvUrl => ImportFocus::CsvNotes,
            ImportFocus::CsvNotes => ImportFocus::CsvTags,
            ImportFocus::CsvTags => ImportFocus::CsvSkipHeader,
            ImportFocus::CsvSkipHeader => ImportFocus::SourceList,
        };
    }

    #[allow(dead_code)]
    fn import_focus_cycle_prev(&mut self) {
        // Simplified: just cycle forward until we wrap. For reverse, we iterate through all.
        let target = self.import_focus;
        let mut current = self.import_focus;
        loop {
            let prev = current;
            self.import_focus_cycle_next();
            // After cycling, if next match would be the target, we stop.
            // Actually, let's just do a simpler approach: iterate all fields to find prev.
            current = self.import_focus;
            if self.import_focus == target {
                self.import_focus = prev;
                break;
            }
        }
    }

    fn reset_for_entry(&mut self) {
        match self.mode {
            ImportExportMode::Import => {
                self.import_step = ImportStep::SourceSelect;
                self.import_focus = ImportFocus::SourceList;
                self.selected_source_idx = 0;
            }
            ImportExportMode::Export => {
                self.export_step = ExportStep::Form;
                self.export_focus = ExportFocus::Scope;
            }
        }
    }

    /// Capture reusable navigation state for this screen (excludes sensitive buffers).
    pub fn to_restore_state(&self) -> crate::tui::state::ImportExportRestoreState {
        crate::tui::state::ImportExportRestoreState {
            mode: self.mode,
            entry_point: self.entry_point,
            import_step: self.import_step,
            selected_source_idx: self.selected_source_idx,
            import_focus: self.import_focus,
            export_step: self.export_step,
            export_focus: self.export_focus,
            export_scope_option: self.export_scope_option,
        }
    }

    /// Restore navigation state from a previously captured restore state.
    /// Clears sensitive password buffers on restore.
    pub fn restore_from(&mut self, restore: crate::tui::state::ImportExportRestoreState) {
        self.mode = restore.mode;
        self.entry_point = restore.entry_point;
        self.import_step = restore.import_step;
        self.selected_source_idx = restore
            .selected_source_idx
            .min(IMPORT_SOURCES.len().saturating_sub(1));
        self.import_focus = restore.import_focus;
        self.export_step = restore.export_step;
        self.export_focus = restore.export_focus;
        self.export_scope_option = restore.export_scope_option;

        // Clear sensitive buffers
        self.decrypt_password.clear();
        self.export_password.clear();
        self.export_confirm_password.clear();
        self.master_password.clear();
    }
}

impl Default for ImportExportScreen {
    fn default() -> Self {
        Self::new()
    }
}

// ── Screen trait impl ───────────────────────────────────────────────────────

impl Screen for ImportExportScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key) => self.handle_key(key, ctx),
            Message::CommandCompleted(result) => self.handle_command_result(result),
            Message::ImportProgress {
                current,
                total,
                current_name,
            } => {
                self.import_progress_current = current;
                self.import_progress_total = total;
                self.import_progress_name = current_name;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut ratatui::Frame, area: Rect) {
        match self.mode {
            ImportExportMode::Import => match self.import_step {
                ImportStep::SourceSelect => self.view_import_source_select(frame, area),
                ImportStep::Preview => self.view_import_preview(frame, area),
                ImportStep::Importing => self.view_importing(frame, area),
                ImportStep::Complete => self.view_import_complete(frame, area),
            },
            ImportExportMode::Export => match self.export_step {
                ExportStep::Form => self.view_export_form(frame, area),
                ExportStep::MasterPasswordConfirm => {
                    self.view_export_master_password_confirm(frame, area)
                }
                ExportStep::Exporting => self.view_exporting(frame, area),
                ExportStep::Complete => self.view_export_complete(frame, area),
            },
        }
    }

    fn on_mount(&mut self, _ctx: &mut ScreenContext) {
        // Reset all sensitive state
        self.file_path.zeroize();
        self.file_path.clear();
        self.decrypt_password.zeroize();
        self.decrypt_password.clear();
        self.export_password.zeroize();
        self.export_password.clear();
        self.export_confirm_password.zeroize();
        self.export_confirm_password.clear();
        self.master_password.zeroize();
        self.master_password.clear();
        self.error_message = None;
        self.preview = None;
        self.import_progress_current = 0;
        self.import_progress_total = 0;
        self.import_progress_name.clear();
        self.imported_count = 0;
        self.skipped_count = 0;
        self.export_result_path = None;
        self.export_record_count = 0;
        self.export_password_strength = None;

        // Only reset navigation state on first entry, not when restored from snapshot
        if self.import_step == ImportStep::SourceSelect
            && self.export_step == ExportStep::Form
            && self.error_message.is_none()
        {
            self.reset_for_entry();
        }
    }

    fn on_unmount(&mut self) {
        self.file_path.zeroize();
        self.decrypt_password.zeroize();
        self.export_password.zeroize();
        self.export_confirm_password.zeroize();
        self.master_password.zeroize();
    }
}

// ── Key handling ────────────────────────────────────────────────────────────

impl ImportExportScreen {
    fn handle_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match self.mode {
            ImportExportMode::Import => self.handle_import_key(key, ctx),
            ImportExportMode::Export => self.handle_export_key(key, ctx),
        }
    }

    fn handle_import_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match self.import_step {
            ImportStep::SourceSelect => self.handle_import_source_select_key(key, ctx),
            ImportStep::Preview => self.handle_import_preview_key(key, ctx),
            ImportStep::Importing => {
                if key.code == KeyCode::Esc {
                    self.go_back()
                } else {
                    ScreenResult::Continue
                }
            }
            ImportStep::Complete => {
                if key.code == KeyCode::Enter || key.code == KeyCode::Esc {
                    self.go_back()
                } else {
                    ScreenResult::Continue
                }
            }
        }
    }

    fn handle_import_source_select_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        self.error_message = None;

        match key.code {
            KeyCode::Esc => return self.go_back(),
            KeyCode::Tab => {
                self.import_focus_cycle_next();
            }
            KeyCode::BackTab => {
                // Cycle backwards by cycling forward through all options
                let fields = [
                    ImportFocus::SourceList,
                    ImportFocus::FilePath,
                    ImportFocus::Password,
                    ImportFocus::CsvName,
                    ImportFocus::CsvUsername,
                    ImportFocus::CsvPassword,
                    ImportFocus::CsvUrl,
                    ImportFocus::CsvNotes,
                    ImportFocus::CsvTags,
                    ImportFocus::CsvSkipHeader,
                ];
                let current_idx = fields
                    .iter()
                    .position(|f| *f == self.import_focus)
                    .unwrap_or(0);
                // Find the previous valid field by cycling backward
                for i in (0..fields.len()).rev() {
                    let idx = (current_idx + fields.len() - i) % fields.len();
                    let candidate = fields[idx];
                    if self.is_import_focus_valid(candidate) {
                        self.import_focus = candidate;
                        break;
                    }
                }
            }
            KeyCode::Up => {
                if self.import_focus == ImportFocus::SourceList && self.selected_source_idx > 0 {
                    self.selected_source_idx -= 1;
                }
            }
            KeyCode::Down => {
                if self.import_focus == ImportFocus::SourceList
                    && self.selected_source_idx < IMPORT_SOURCES.len() - 1
                {
                    self.selected_source_idx += 1;
                }
            }
            KeyCode::Enter => {
                if self.import_focus == ImportFocus::SourceList {
                    self.import_focus = ImportFocus::FilePath;
                } else if self.import_focus == ImportFocus::CsvSkipHeader {
                    self.csv_mapping.skip_header = !self.csv_mapping.skip_header;
                } else if self.import_focus == ImportFocus::Password
                    && self.current_source() == ImportSource::Csv
                {
                    // Skip password for CSV, go to csv fields
                    self.import_focus = ImportFocus::CsvName;
                } else {
                    // Validate and trigger import validation
                    return self.trigger_validate_import(ctx);
                }
            }
            KeyCode::Char(c) => match self.import_focus {
                ImportFocus::FilePath => self.file_path.push(c),
                ImportFocus::Password => self.decrypt_password.push(c),
                ImportFocus::CsvName => self.csv_mapping.name_column.push(c),
                ImportFocus::CsvUsername => self.csv_mapping.username_column.push(c),
                ImportFocus::CsvPassword => self.csv_mapping.password_column.push(c),
                ImportFocus::CsvUrl => self.csv_mapping.url_column.push(c),
                ImportFocus::CsvNotes => self.csv_mapping.notes_column.push(c),
                ImportFocus::CsvTags => {
                    if let Some(ref mut tags) = self.csv_mapping.tags_column {
                        tags.push(c);
                    } else {
                        self.csv_mapping.tags_column = Some(c.to_string());
                    }
                }
                _ => {}
            },
            KeyCode::Backspace => match self.import_focus {
                ImportFocus::FilePath => {
                    self.file_path.pop();
                }
                ImportFocus::Password => {
                    self.decrypt_password.pop();
                }
                ImportFocus::CsvName => {
                    self.csv_mapping.name_column.pop();
                }
                ImportFocus::CsvUsername => {
                    self.csv_mapping.username_column.pop();
                }
                ImportFocus::CsvPassword => {
                    self.csv_mapping.password_column.pop();
                }
                ImportFocus::CsvUrl => {
                    self.csv_mapping.url_column.pop();
                }
                ImportFocus::CsvNotes => {
                    self.csv_mapping.notes_column.pop();
                }
                ImportFocus::CsvTags => {
                    if let Some(ref mut tags) = self.csv_mapping.tags_column {
                        tags.pop();
                        if tags.is_empty() {
                            self.csv_mapping.tags_column = None;
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
        ScreenResult::Continue
    }

    fn is_import_focus_valid(&self, focus: ImportFocus) -> bool {
        let is_csv = self.current_source() == ImportSource::Csv;
        let has_password = source_needs_password(self.current_source());
        match focus {
            ImportFocus::SourceList | ImportFocus::FilePath => true,
            ImportFocus::Password => has_password,
            ImportFocus::CsvName
            | ImportFocus::CsvUsername
            | ImportFocus::CsvPassword
            | ImportFocus::CsvUrl
            | ImportFocus::CsvNotes
            | ImportFocus::CsvTags
            | ImportFocus::CsvSkipHeader => is_csv,
        }
    }

    fn trigger_validate_import(&mut self, ctx: &mut ScreenContext) -> ScreenResult {
        if self.file_path.is_empty() {
            self.error_message = Some("File path is required".to_string());
            return ScreenResult::Continue;
        }

        let source = self.current_source();
        self.source = Some(source);

        if source_needs_password(source) && self.decrypt_password.is_empty() {
            self.error_message = Some("Password is required for this source".to_string());
            return ScreenResult::Continue;
        }

        let _column_mapping = if source == ImportSource::Csv {
            Some(self.csv_mapping.clone())
        } else {
            None
        };

        let password = if self.decrypt_password.is_empty() {
            None
        } else {
            Some(SecureStr::new(self.decrypt_password.clone()))
        };

        let cmd = Command::ValidateImportFile {
            source,
            path: PathBuf::from(&self.file_path),
            password,
        };
        let _ = ctx.command_tx.try_send(cmd);

        // Store column_mapping for later use in ExecuteImport
        self.import_step = ImportStep::Preview;
        ScreenResult::Continue
    }

    fn handle_import_preview_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match key.code {
            KeyCode::Esc => {
                self.import_step = ImportStep::SourceSelect;
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                let source = match self.source {
                    Some(s) => s,
                    None => return ScreenResult::Continue,
                };

                let column_mapping = if source == ImportSource::Csv {
                    Some(self.csv_mapping.clone())
                } else {
                    None
                };

                let password = if self.decrypt_password.is_empty() {
                    None
                } else {
                    Some(SecureStr::new(self.decrypt_password.clone()))
                };

                let cmd = Command::ExecuteImport {
                    source,
                    path: PathBuf::from(&self.file_path),
                    password,
                    column_mapping,
                    import_as_notes: false,
                };
                let _ = ctx.command_tx.try_send(cmd);
                self.import_step = ImportStep::Importing;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_export_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match self.export_step {
            ExportStep::Form => self.handle_export_form_key(key),
            ExportStep::MasterPasswordConfirm => self.handle_export_master_password_key(key, ctx),
            ExportStep::Exporting => ScreenResult::Continue,
            ExportStep::Complete => {
                if key.code == KeyCode::Enter || key.code == KeyCode::Esc {
                    self.go_back()
                } else {
                    ScreenResult::Continue
                }
            }
        }
    }

    fn handle_export_form_key(&mut self, key: KeyEvent) -> ScreenResult {
        self.error_message = None;

        match key.code {
            KeyCode::Esc => return self.go_back(),
            KeyCode::Tab => {
                self.export_focus = match self.export_focus {
                    ExportFocus::Scope => ExportFocus::ExportPassword,
                    ExportFocus::ExportPassword => ExportFocus::ConfirmPassword,
                    ExportFocus::ConfirmPassword => ExportFocus::OutputPath,
                    ExportFocus::OutputPath => ExportFocus::Scope,
                };
            }
            KeyCode::BackTab => {
                self.export_focus = match self.export_focus {
                    ExportFocus::Scope => ExportFocus::OutputPath,
                    ExportFocus::ExportPassword => ExportFocus::Scope,
                    ExportFocus::ConfirmPassword => ExportFocus::ExportPassword,
                    ExportFocus::OutputPath => ExportFocus::ConfirmPassword,
                };
            }
            KeyCode::Up if self.export_focus == ExportFocus::Scope => {
                self.export_scope_option = match self.export_scope_option {
                    ExportScopeOption::All => ExportScopeOption::All,
                    ExportScopeOption::CurrentFilter => ExportScopeOption::All,
                    ExportScopeOption::ByTag => ExportScopeOption::CurrentFilter,
                };
            }
            KeyCode::Down if self.export_focus == ExportFocus::Scope => {
                self.export_scope_option = match self.export_scope_option {
                    ExportScopeOption::All => ExportScopeOption::CurrentFilter,
                    ExportScopeOption::CurrentFilter => ExportScopeOption::ByTag,
                    ExportScopeOption::ByTag => ExportScopeOption::ByTag,
                };
            }
            KeyCode::Enter => {
                if self.export_focus == ExportFocus::Scope {
                    self.export_focus = ExportFocus::ExportPassword;
                } else {
                    return self.validate_export_form();
                }
            }
            KeyCode::Char(c) => match self.export_focus {
                ExportFocus::ExportPassword => {
                    self.export_password.push(c);
                    self.update_export_strength();
                }
                ExportFocus::ConfirmPassword => {
                    self.export_confirm_password.push(c);
                }
                ExportFocus::OutputPath => {
                    self.export_output_path.push(c);
                }
                ExportFocus::Scope => {}
            },
            KeyCode::Backspace => match self.export_focus {
                ExportFocus::ExportPassword => {
                    self.export_password.pop();
                    self.update_export_strength();
                }
                ExportFocus::ConfirmPassword => {
                    self.export_confirm_password.pop();
                }
                ExportFocus::OutputPath => {
                    self.export_output_path.pop();
                }
                ExportFocus::Scope => {}
            },
            _ => {}
        }
        ScreenResult::Continue
    }

    fn validate_export_form(&mut self) -> ScreenResult {
        if self.export_password.len() < 8 {
            self.error_message = Some("Password must be at least 8 characters".to_string());
            return ScreenResult::Continue;
        }
        if self.export_password != self.export_confirm_password {
            self.error_message = Some("Passwords do not match".to_string());
            return ScreenResult::Continue;
        }
        if self.export_output_path.is_empty() {
            self.error_message = Some("Output path is required".to_string());
            return ScreenResult::Continue;
        }
        self.error_message = None;
        self.export_step = ExportStep::MasterPasswordConfirm;
        ScreenResult::Continue
    }

    fn handle_export_master_password_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        self.error_message = None;

        match key.code {
            KeyCode::Esc => {
                self.export_step = ExportStep::Form;
                ScreenResult::Continue
            }
            KeyCode::Backspace => {
                self.master_password.pop();
                ScreenResult::Continue
            }
            KeyCode::Char(c) => {
                self.master_password.push(c);
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                if self.master_password.is_empty() {
                    self.error_message =
                        Some("Master password is required to authorize export".to_string());
                    return ScreenResult::Continue;
                }

                let scope = match self.export_scope_option {
                    ExportScopeOption::All => ExportScope::All,
                    ExportScopeOption::CurrentFilter => {
                        ExportScope::CurrentFilter(crate::commands::types::RecordFilter::All)
                    }
                    ExportScopeOption::ByTag => ExportScope::ByTag(String::new()),
                };

                let export_pw = std::mem::take(&mut self.export_password);
                let master_pw = std::mem::take(&mut self.master_password);
                self.export_confirm_password.zeroize();
                self.export_confirm_password.clear();

                let cmd = Command::ExecuteExport {
                    scope,
                    output_path: PathBuf::from(&self.export_output_path),
                    export_password: SecureStr::new(export_pw),
                    master_password: SecureStr::new(master_pw),
                };
                let _ = ctx.command_tx.try_send(cmd);
                self.export_step = ExportStep::Exporting;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::ImportValidated { preview } => {
                self.preview = Some(preview);
                ScreenResult::Continue
            }
            CommandResult::ImportCompleted {
                imported_count,
                skipped_count,
            } => {
                self.imported_count = imported_count;
                self.skipped_count = skipped_count;
                self.import_step = ImportStep::Complete;
                ScreenResult::Continue
            }
            CommandResult::ExportCompleted { path, record_count } => {
                self.export_result_path = Some(path);
                self.export_record_count = record_count;
                self.export_step = ExportStep::Complete;
                ScreenResult::Continue
            }
            CommandResult::Error { fallback, .. } => {
                self.error_message = Some(fallback);
                // Go back to the appropriate step on error
                if self.mode == ImportExportMode::Import {
                    if self.import_step == ImportStep::Importing {
                        self.import_step = ImportStep::Preview;
                    } else if self.import_step == ImportStep::Preview {
                        self.import_step = ImportStep::SourceSelect;
                    }
                } else if self.mode == ImportExportMode::Export {
                    if self.export_step == ExportStep::Exporting {
                        self.export_step = ExportStep::MasterPasswordConfirm;
                    } else if self.export_step == ExportStep::MasterPasswordConfirm {
                        self.export_step = ExportStep::Form;
                    }
                }
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}
