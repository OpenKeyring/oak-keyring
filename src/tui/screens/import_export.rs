//! Import/Export screen — import from external sources or export to backup files.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use zeroize::Zeroize;

use crate::commands::result::CommandResult;
use crate::commands::types::{
    CsvColumnMapping, ExportScope, ImportPreview, ImportSource, Screen as ScreenEnum,
};
use crate::commands::{Command, Message};
use crate::crypto::strength::{evaluate_strength, PasswordStrength, StrengthLevel};
use crate::tui::theme::{
    self, Styles, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED, TEXT_PLACEHOLDER, TEXT_SECONDARY,
    WARNING,
};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::SecureStr;

// ── Enums ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportExportMode {
    Import,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStep {
    SourceSelect,
    Preview,
    Importing,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportStep {
    Form,
    MasterPasswordConfirm,
    Exporting,
    Complete,
}

/// Focus targets within the Import SourceSelect step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFocus {
    SourceList,
    FilePath,
    Password,
    // CSV column mapping fields
    CsvName,
    CsvUsername,
    CsvPassword,
    CsvUrl,
    CsvNotes,
    CsvTags,
    CsvSkipHeader,
}

/// Focus targets within the Export Form step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFocus {
    Scope,
    ExportPassword,
    ConfirmPassword,
    OutputPath,
}

/// Export scope selection index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportScopeOption {
    All,
    CurrentFilter,
    ByTag,
}

// ── Source metadata ─────────────────────────────────────────────────────────

const IMPORT_SOURCES: [(ImportSource, &str, bool); 6] = [
    (ImportSource::KeePass, "KeePass (.kdbx)", true),
    (ImportSource::OnePassword1pux, "1Password (.1pux)", false),
    (
        ImportSource::OnePasswordOpvault,
        "1Password (.opvault)",
        true,
    ),
    (ImportSource::Bitwarden, "Bitwarden (.json)", true),
    (ImportSource::Csv, "CSV", false),
    (
        ImportSource::OpenKeyringBackup,
        "OpenKeyring Backup (.okb)",
        false,
    ),
];

#[allow(dead_code)]
fn source_display(source: ImportSource) -> &'static str {
    IMPORT_SOURCES
        .iter()
        .find(|(s, _, _)| *s == source)
        .map(|(_, name, _)| *name)
        .unwrap_or("Unknown")
}

fn source_needs_password(source: ImportSource) -> bool {
    IMPORT_SOURCES
        .iter()
        .find(|(s, _, _)| *s == source)
        .map(|(_, _, pw)| *pw)
        .unwrap_or(false)
}

fn default_export_path() -> String {
    dirs::document_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("keyring-backup.okb")
        .to_string_lossy()
        .to_string()
}

// ── ImportExportScreen ──────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ImportExportScreen {
    pub mode: ImportExportMode,

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

    fn display_password(password: &str) -> String {
        "\u{2022}".repeat(password.len())
    }

    fn strength_color(level: &StrengthLevel) -> ratatui::style::Color {
        match level {
            StrengthLevel::VeryWeak | StrengthLevel::Weak => ERROR,
            StrengthLevel::Fair => WARNING,
            StrengthLevel::Strong => PRIMARY,
            StrengthLevel::VeryStrong => SUCCESS,
        }
    }

    fn update_export_strength(&mut self) {
        if self.export_password.is_empty() {
            self.export_password_strength = None;
        } else {
            self.export_password_strength = Some(evaluate_strength(&self.export_password));
        }
    }

    fn current_source(&self) -> ImportSource {
        IMPORT_SOURCES[self.selected_source_idx].0
    }

    fn import_focus_cycle_next(&mut self) {
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

        if self.mode == ImportExportMode::Import {
            self.import_step = ImportStep::SourceSelect;
            self.import_focus = ImportFocus::SourceList;
            self.selected_source_idx = 0;
            self.source = None;
        } else {
            self.export_step = ExportStep::Form;
            self.export_focus = ExportFocus::Scope;
            self.export_scope_option = ExportScopeOption::All;
            self.export_output_path = default_export_path();
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
                    // Future: send cancel command
                    ScreenResult::Continue
                } else {
                    ScreenResult::Continue
                }
            }
            ImportStep::Complete => {
                if key.code == KeyCode::Enter || key.code == KeyCode::Esc {
                    ScreenResult::NavigateTo(ScreenEnum::Config)
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
            KeyCode::Esc => return ScreenResult::NavigateTo(ScreenEnum::Config),
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
                    ScreenResult::NavigateTo(ScreenEnum::Config)
                } else {
                    ScreenResult::Continue
                }
            }
        }
    }

    fn handle_export_form_key(&mut self, key: KeyEvent) -> ScreenResult {
        self.error_message = None;

        match key.code {
            KeyCode::Esc => return ScreenResult::NavigateTo(ScreenEnum::Config),
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

// ── View: Import SourceSelect ───────────────────────────────────────────────

impl ImportExportScreen {
    fn view_import_source_select(&self, frame: &mut ratatui::Frame, area: Rect) {
        // Vertical centering
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(20),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(60),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        // Title
        let title = Paragraph::new("Import Data")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        // Mode switch hint
        let mode_hint =
            Paragraph::new("Tab: switch fields | \u{2191}\u{2193}: navigate | 1=Import 2=Export")
                .style(ratatui::style::Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);

        // Source list header
        let source_header =
            Paragraph::new("Select Source:").style(ratatui::style::Style::default().fg(TEXT));

        // Source items
        let source_items: Vec<ratatui::text::Line> = IMPORT_SOURCES
            .iter()
            .enumerate()
            .map(|(i, (_, name, needs_pw))| {
                let suffix = if *needs_pw { " (password)" } else { "" };
                let label = format!("  {}{}{}", name, suffix, "");
                let style = if i == self.selected_source_idx
                    && self.import_focus == ImportFocus::SourceList
                {
                    Styles::selected_focused()
                } else if i == self.selected_source_idx {
                    ratatui::style::Style::default().fg(PRIMARY)
                } else {
                    ratatui::style::Style::default().fg(TEXT_SECONDARY)
                };
                ratatui::text::Line::from(ratatui::text::Span::styled(label, style))
            })
            .collect();

        let source_list = Paragraph::new(source_items);

        // File path field
        let file_border = if self.import_focus == ImportFocus::FilePath {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let file_block = Block::default()
            .borders(Borders::ALL)
            .border_style(file_border)
            .title(" File Path ");
        let file_display = if self.file_path.is_empty() {
            let placeholder = "/path/to/import/file";
            Paragraph::new(placeholder).style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(&*self.file_path).style(ratatui::style::Style::default().fg(TEXT))
        };

        // Password field (only shown for sources that need it)
        let needs_password = source_needs_password(self.current_source());
        let password_block_maybe = if needs_password {
            let pw_border = if self.import_focus == ImportFocus::Password {
                Styles::focused_border()
            } else {
                Styles::unfocused_border()
            };
            Some(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(pw_border)
                    .title(" Decryption Password "),
            )
        } else {
            None
        };
        let pw_display_maybe = if needs_password {
            if self.decrypt_password.is_empty() {
                Some(
                    Paragraph::new("Enter password")
                        .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER)),
                )
            } else {
                Some(
                    Paragraph::new(Self::display_password(&self.decrypt_password))
                        .style(ratatui::style::Style::default().fg(TEXT)),
                )
            }
        } else {
            None
        };

        // Error
        let error_line = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg)).style(Styles::error_text())
        });

        // Hint
        let hint = Paragraph::new("Enter: validate | Esc: back")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);

        // CSV mapping section
        let is_csv = self.current_source() == ImportSource::Csv;
        let csv_row_count = if is_csv { 8 } else { 0 }; // 6 fields + skip header + header label

        // Calculate row constraints
        let mut constraints = vec![
            Constraint::Length(1), // title
            Constraint::Length(1), // mode hint
            Constraint::Length(1), // gap
            Constraint::Length(1), // source header
            Constraint::Length(6), // source list (6 items)
            Constraint::Length(1), // gap
            Constraint::Length(3), // file path
        ];

        if needs_password {
            constraints.push(Constraint::Length(1)); // gap
            constraints.push(Constraint::Length(3)); // password
        }

        if is_csv {
            for _ in 0..csv_row_count {
                constraints.push(Constraint::Length(1));
            }
        }

        constraints.push(Constraint::Length(1)); // error or gap
        constraints.push(Constraint::Length(1)); // gap
        constraints.push(Constraint::Length(1)); // hint

        let rows = Layout::vertical(constraints).split(content_area);

        let mut row_idx = 0;
        frame.render_widget(title, rows[row_idx]);
        row_idx += 1;
        frame.render_widget(mode_hint, rows[row_idx]);
        row_idx += 1;
        row_idx += 1; // gap
        frame.render_widget(source_header, rows[row_idx]);
        row_idx += 1;
        frame.render_widget(source_list, rows[row_idx]);
        row_idx += 1;
        row_idx += 1; // gap
        frame.render_widget(file_block, rows[row_idx]);
        let file_inner = Layout::vertical([Constraint::Length(1)]).split(rows[row_idx])[0];
        let file_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(file_inner);
        frame.render_widget(file_display, file_padded[1]);
        row_idx += 1;

        if needs_password {
            row_idx += 1; // gap
            if let Some(ref block) = password_block_maybe {
                frame.render_widget(block.clone(), rows[row_idx]);
                let pw_inner = Layout::vertical([Constraint::Length(1)]).split(rows[row_idx])[0];
                let pw_padded = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)])
                    .split(pw_inner);
                if let Some(ref display) = pw_display_maybe {
                    frame.render_widget(display.clone(), pw_padded[1]);
                }
            }
            row_idx += 1;
        }

        if is_csv {
            // CSV column mapping header
            let csv_header =
                Paragraph::new("Column Mapping:").style(ratatui::style::Style::default().fg(TEXT));
            frame.render_widget(csv_header, rows[row_idx]);
            row_idx += 1;

            // CSV fields
            let csv_fields: Vec<(&str, &str, ImportFocus)> = vec![
                (
                    "Name column:",
                    &self.csv_mapping.name_column,
                    ImportFocus::CsvName,
                ),
                (
                    "Username column:",
                    &self.csv_mapping.username_column,
                    ImportFocus::CsvUsername,
                ),
                (
                    "Password column:",
                    &self.csv_mapping.password_column,
                    ImportFocus::CsvPassword,
                ),
                (
                    "URL column:",
                    &self.csv_mapping.url_column,
                    ImportFocus::CsvUrl,
                ),
                (
                    "Notes column:",
                    &self.csv_mapping.notes_column,
                    ImportFocus::CsvNotes,
                ),
                (
                    "Tags column:",
                    self.csv_mapping.tags_column.as_deref().unwrap_or("(none)"),
                    ImportFocus::CsvTags,
                ),
            ];

            for (label, value, focus) in csv_fields {
                let style = if self.import_focus == focus {
                    ratatui::style::Style::default().fg(PRIMARY)
                } else {
                    ratatui::style::Style::default().fg(TEXT_SECONDARY)
                };
                let line = Paragraph::new(format!("  {}: {}", label, value)).style(style);
                frame.render_widget(line, rows[row_idx]);
                row_idx += 1;
            }

            // Skip header toggle
            let skip_style = if self.import_focus == ImportFocus::CsvSkipHeader {
                ratatui::style::Style::default().fg(PRIMARY)
            } else {
                ratatui::style::Style::default().fg(TEXT_SECONDARY)
            };
            let checkbox = if self.csv_mapping.skip_header {
                "[x]"
            } else {
                "[ ]"
            };
            let skip_line =
                Paragraph::new(format!("  {} Skip header row", checkbox)).style(skip_style);
            frame.render_widget(skip_line, rows[row_idx]);
            row_idx += 1;
        }

        // Error
        if let Some(ref el) = error_line {
            if row_idx < rows.len() {
                frame.render_widget(el.clone(), rows[row_idx]);
            }
        }
        row_idx += 1;

        // Hint (second to last = gap, last = hint)
        if row_idx + 1 < rows.len() {
            frame.render_widget(hint, rows[row_idx + 1]);
        }
    }
}

// ── View: Import Preview ────────────────────────────────────────────────────

impl ImportExportScreen {
    fn view_import_preview(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(10),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(60),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        let title = Paragraph::new("Import Preview")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        let mut constraints = vec![
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
        ];

        if let Some(ref preview) = self.preview {
            // Summary
            constraints.push(Constraint::Length(1)); // importable
            constraints.push(Constraint::Length(1)); // needs review
            constraints.push(Constraint::Length(1)); // failed
            constraints.push(Constraint::Length(1)); // gap

            // Review items
            let review_count = preview.review_items.len().min(5);
            for _ in 0..review_count {
                constraints.push(Constraint::Length(1));
            }
            if preview.review_items.len() > 5 {
                constraints.push(Constraint::Length(1)); // "...and more"
            }

            // Failed items
            let failed_count = preview.failed_items.len().min(5);
            if failed_count > 0 {
                constraints.push(Constraint::Length(1)); // gap
                constraints.push(Constraint::Length(1)); // header
                for _ in 0..failed_count {
                    constraints.push(Constraint::Length(1));
                }
            }
        }

        constraints.push(Constraint::Length(1)); // gap
        constraints.push(Constraint::Length(1)); // hint

        let rows = Layout::vertical(constraints).split(content_area);

        let mut row_idx = 0;
        frame.render_widget(title, rows[row_idx]);
        row_idx += 1;
        row_idx += 1; // gap

        if let Some(ref preview) = self.preview {
            let importable_line = Paragraph::new(format!(
                "{} Importable: {}",
                theme::ICON_SUCCESS,
                preview.importable
            ))
            .style(Styles::success_text());
            frame.render_widget(importable_line, rows[row_idx]);
            row_idx += 1;

            let review_line = Paragraph::new(format!(
                "{} Needs review: {}",
                theme::ICON_WARNING,
                preview.needs_review
            ))
            .style(Styles::warning_text());
            frame.render_widget(review_line, rows[row_idx]);
            row_idx += 1;

            let failed_line =
                Paragraph::new(format!("{} Failed: {}", theme::ICON_ERROR, preview.failed))
                    .style(Styles::error_text());
            frame.render_widget(failed_line, rows[row_idx]);
            row_idx += 1;

            row_idx += 1; // gap

            // Review items
            if !preview.review_items.is_empty() {
                let header = Paragraph::new("Review items:")
                    .style(ratatui::style::Style::default().fg(TEXT));
                frame.render_widget(header, rows[row_idx]);
                row_idx += 1;

                for item in preview.review_items.iter().take(5) {
                    let line = Paragraph::new(format!("  - {} ({})", item.name, item.reason))
                        .style(ratatui::style::Style::default().fg(TEXT_SECONDARY));
                    frame.render_widget(line, rows[row_idx]);
                    row_idx += 1;
                }
                if preview.review_items.len() > 5 {
                    let more =
                        Paragraph::new(format!("  ...and {} more", preview.review_items.len() - 5))
                            .style(ratatui::style::Style::default().fg(TEXT_MUTED));
                    frame.render_widget(more, rows[row_idx]);
                    row_idx += 1;
                }
            }

            // Failed items
            if !preview.failed_items.is_empty() {
                row_idx += 1; // gap
                let header = Paragraph::new("Failed items:").style(Styles::error_text());
                frame.render_widget(header, rows[row_idx]);
                row_idx += 1;

                for item in preview.failed_items.iter().take(5) {
                    let line = Paragraph::new(format!("  - {} ({})", item.name, item.reason))
                        .style(Styles::error_text());
                    frame.render_widget(line, rows[row_idx]);
                    row_idx += 1;
                }
            }
        }

        // Hint
        let hint = Paragraph::new("Enter: start import | Esc: back")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        if row_idx < rows.len() {
            frame.render_widget(hint, rows[row_idx]);
        }
    }
}

// ── View: Importing progress ───────────────────────────────────────────────

impl ImportExportScreen {
    fn view_importing(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(6),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        let title = Paragraph::new("Importing...")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        // Progress bar
        let total = if self.import_progress_total > 0 {
            self.import_progress_total
        } else {
            1
        };
        let ratio = self.import_progress_current as f64 / total as f64;
        let bar_width = 40usize;
        let filled = (ratio * bar_width as f64).round() as usize;
        let empty = bar_width - filled;
        let bar_str = format!(
            "[{}{}] {}/{}",
            "\u{2588}".repeat(filled),
            "\u{2591}".repeat(empty),
            self.import_progress_current,
            total,
        );
        let progress_bar =
            Paragraph::new(bar_str).style(ratatui::style::Style::default().fg(PRIMARY));

        // Current item
        let current_item = if self.import_progress_name.is_empty() {
            Paragraph::new("").style(ratatui::style::Style::default().fg(TEXT_MUTED))
        } else {
            Paragraph::new(format!("Processing: {}", self.import_progress_name))
                .style(ratatui::style::Style::default().fg(TEXT_SECONDARY))
        };

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // progress bar
            Constraint::Length(1), // current item
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(progress_bar, rows[2]);
        frame.render_widget(current_item, rows[3]);

        let hint = Paragraph::new("Please wait...")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[5]);
    }
}

// ── View: Import Complete ──────────────────────────────────────────────────

impl ImportExportScreen {
    fn view_import_complete(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(8),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        let title = Paragraph::new("Import Complete")
            .style(Styles::success_text())
            .alignment(Alignment::Center);

        let imported_line = Paragraph::new(format!(
            "{} Records imported: {}",
            theme::ICON_SUCCESS,
            self.imported_count
        ))
        .style(Styles::success_text());

        let skipped_line = if self.skipped_count > 0 {
            Paragraph::new(format!(
                "{} Records skipped: {}",
                theme::ICON_WARNING,
                self.skipped_count
            ))
            .style(Styles::warning_text())
        } else {
            Paragraph::new("").style(ratatui::style::Style::default().fg(TEXT_MUTED))
        };

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // imported
            Constraint::Length(1), // skipped
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(imported_line, rows[2]);
        frame.render_widget(skipped_line, rows[3]);

        let hint = Paragraph::new("Enter: back to config")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[5]);
    }
}

// ── View: Export Form ──────────────────────────────────────────────────────

impl ImportExportScreen {
    fn view_export_form(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(20),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        // Title
        let title = Paragraph::new("Export Data")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        // Scope selector
        let scope_header =
            Paragraph::new("Export Scope:").style(ratatui::style::Style::default().fg(TEXT));
        let scope_options = [
            ("All records", ExportScopeOption::All),
            ("Current filter", ExportScopeOption::CurrentFilter),
            ("By tag", ExportScopeOption::ByTag),
        ];
        let scope_items: Vec<ratatui::text::Line> = scope_options
            .iter()
            .map(|(label, opt)| {
                let is_selected = self.export_scope_option == *opt;
                let marker = if is_selected { ">" } else { " " };
                let style = if is_selected && self.export_focus == ExportFocus::Scope {
                    Styles::selected_focused()
                } else if is_selected {
                    ratatui::style::Style::default().fg(PRIMARY)
                } else {
                    ratatui::style::Style::default().fg(TEXT_SECONDARY)
                };
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    format!(" {} {}", marker, label),
                    style,
                ))
            })
            .collect();
        let scope_list = Paragraph::new(scope_items);

        // Export password
        let pw_border = if self.export_focus == ExportFocus::ExportPassword {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let pw_block = Block::default()
            .borders(Borders::ALL)
            .border_style(pw_border)
            .title(" Export Password ");
        let pw_display = if self.export_password.is_empty() {
            Paragraph::new("Enter export password")
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(Self::display_password(&self.export_password))
                .style(ratatui::style::Style::default().fg(TEXT))
        };

        // Strength bar
        let strength_line = if let Some(ref s) = self.export_password_strength {
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

        // Confirm password
        let confirm_border = if self.export_focus == ExportFocus::ConfirmPassword {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let confirm_block = Block::default()
            .borders(Borders::ALL)
            .border_style(confirm_border)
            .title(" Confirm Password ");
        let confirm_display = if self.export_confirm_password.is_empty() {
            Paragraph::new("Confirm export password")
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(Self::display_password(&self.export_confirm_password))
                .style(ratatui::style::Style::default().fg(TEXT))
        };

        // Match indicator
        let match_line =
            if !self.export_password.is_empty() && !self.export_confirm_password.is_empty() {
                if self.export_password == self.export_confirm_password {
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

        // Output path
        let path_border = if self.export_focus == ExportFocus::OutputPath {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let path_block = Block::default()
            .borders(Borders::ALL)
            .border_style(path_border)
            .title(" Output Path ");
        let path_display = if self.export_output_path.is_empty() {
            Paragraph::new("~/Documents/keyring-backup.okb")
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(&*self.export_output_path)
                .style(ratatui::style::Style::default().fg(TEXT))
        };

        // Error
        let error_line = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg)).style(Styles::error_text())
        });

        // Hint
        let hint =
            Paragraph::new("Tab: switch fields | \u{2191}\u{2193}: select scope | Esc: back")
                .style(ratatui::style::Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // scope header
            Constraint::Length(3), // scope list
            Constraint::Length(3), // export password
            Constraint::Length(1), // strength bar
            Constraint::Length(3), // confirm password
            Constraint::Length(1), // match indicator
            Constraint::Length(1), // gap
            Constraint::Length(3), // output path
            Constraint::Length(1), // error or gap
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        // gap at rows[1]
        frame.render_widget(scope_header, rows[2]);
        frame.render_widget(scope_list, rows[3]);

        // Export password
        frame.render_widget(pw_block, rows[4]);
        let pw_inner = Layout::vertical([Constraint::Length(1)]).split(rows[4])[0];
        let pw_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(pw_inner);
        frame.render_widget(pw_display, pw_padded[1]);

        // Strength
        frame.render_widget(strength_line, rows[5]);

        // Confirm password
        frame.render_widget(confirm_block, rows[6]);
        let confirm_inner = Layout::vertical([Constraint::Length(1)]).split(rows[6])[0];
        let confirm_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(confirm_inner);
        frame.render_widget(confirm_display, confirm_padded[1]);

        // Match indicator
        if let Some(ref ml) = match_line {
            frame.render_widget(ml.clone(), rows[7]);
        }

        // Output path
        frame.render_widget(path_block, rows[9]);
        let path_inner = Layout::vertical([Constraint::Length(1)]).split(rows[9])[0];
        let path_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(path_inner);
        frame.render_widget(path_display, path_padded[1]);

        // Error
        if let Some(ref el) = error_line {
            frame.render_widget(el.clone(), rows[10]);
        }

        // Hint
        frame.render_widget(hint, rows[12]);
    }
}

// ── View: Export Master Password Confirm ───────────────────────────────────

impl ImportExportScreen {
    fn view_export_master_password_confirm(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(12),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        let title = Paragraph::new("Authorize Export")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        let subtitle = Paragraph::new(format!(
            "Export {} to: {}",
            match self.export_scope_option {
                ExportScopeOption::All => "all records",
                ExportScopeOption::CurrentFilter => "filtered records",
                ExportScopeOption::ByTag => "tagged records",
            },
            self.export_output_path,
        ))
        .style(ratatui::style::Style::default().fg(TEXT_SECONDARY))
        .alignment(Alignment::Center);

        // Master password input
        let pw_border = Styles::focused_border();
        let pw_block = Block::default()
            .borders(Borders::ALL)
            .border_style(pw_border)
            .title(" Master Password ");

        let pw_display = if self.master_password.is_empty() {
            Paragraph::new("Enter master password to authorize")
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(Self::display_password(&self.master_password))
                .style(ratatui::style::Style::default().fg(TEXT))
        };

        // Error
        let error_line = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg)).style(Styles::error_text())
        });

        let hint = Paragraph::new("Enter: export | Esc: back")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // subtitle
            Constraint::Length(1), // gap
            Constraint::Length(3), // password input
            Constraint::Length(1), // error or gap
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(subtitle, rows[2]);

        frame.render_widget(pw_block, rows[4]);
        let pw_inner = Layout::vertical([Constraint::Length(1)]).split(rows[4])[0];
        let pw_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(pw_inner);
        frame.render_widget(pw_display, pw_padded[1]);

        if let Some(ref el) = error_line {
            frame.render_widget(el.clone(), rows[5]);
        }

        frame.render_widget(hint, rows[7]);
    }
}

// ── View: Exporting ─────────────────────────────────────────────────────────

impl ImportExportScreen {
    fn view_exporting(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(6),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        let title = Paragraph::new("Exporting...")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        let progress = Paragraph::new("Encrypting and writing export file...")
            .style(ratatui::style::Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);

        let hint = Paragraph::new("Please wait...")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // progress text
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(progress, rows[2]);
        frame.render_widget(hint, rows[4]);
    }
}

// ── View: Export Complete ──────────────────────────────────────────────────

impl ImportExportScreen {
    fn view_export_complete(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(8),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        let title = Paragraph::new("Export Complete")
            .style(Styles::success_text())
            .alignment(Alignment::Center);

        let path_display = self
            .export_result_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| self.export_output_path.clone());

        let path_line = Paragraph::new(format!(
            "{} Saved to: {}",
            theme::ICON_SUCCESS,
            path_display
        ))
        .style(Styles::success_text())
        .wrap(Wrap { trim: true });

        let count_line = Paragraph::new(format!(
            "{} Records exported: {}",
            theme::ICON_SUCCESS,
            self.export_record_count
        ))
        .style(Styles::success_text());

        let hint = Paragraph::new("Enter: back to config")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(2), // path (might wrap)
            Constraint::Length(1), // count
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(path_line, rows[2]);
        frame.render_widget(count_line, rows[3]);
        frame.render_widget(hint, rows[5]);
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::traits::screen::Screen as ScreenTrait;

    #[test]
    fn new_screen_defaults_to_import() {
        let screen = ImportExportScreen::new();
        assert_eq!(screen.mode, ImportExportMode::Import);
        assert_eq!(screen.import_step, ImportStep::SourceSelect);
        assert_eq!(screen.export_step, ExportStep::Form);
        assert!(screen.file_path.is_empty());
        assert!(screen.decrypt_password.is_empty());
        assert!(screen.error_message.is_none());
        assert!(screen.preview.is_none());
    }

    #[test]
    fn on_mount_resets_state() {
        let mut screen = ImportExportScreen::new();
        screen.file_path = "/some/path".to_string();
        screen.decrypt_password = "secret".to_string();
        screen.error_message = Some("error".to_string());

        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let config = crate::config::AppConfig::default();
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &config,
        };
        ScreenTrait::on_mount(&mut screen, &mut ctx);

        assert!(screen.file_path.is_empty());
        assert!(screen.decrypt_password.is_empty());
        assert!(screen.error_message.is_none());
        assert_eq!(screen.import_step, ImportStep::SourceSelect);
    }

    #[test]
    fn on_unmount_clears_sensitive() {
        let mut screen = ImportExportScreen::new();
        screen.file_path = "sensitive_path".to_string();
        screen.decrypt_password = "sensitive_pw".to_string();
        screen.export_password = "export_pw".to_string();
        screen.export_confirm_password = "confirm_pw".to_string();
        screen.master_password = "master_pw".to_string();

        ScreenTrait::on_unmount(&mut screen);

        assert!(screen.file_path.is_empty());
        assert!(screen.decrypt_password.is_empty());
        assert!(screen.export_password.is_empty());
        assert!(screen.export_confirm_password.is_empty());
        assert!(screen.master_password.is_empty());
    }

    #[test]
    fn source_needs_password_correct() {
        assert!(source_needs_password(ImportSource::KeePass));
        assert!(!source_needs_password(ImportSource::OnePassword1pux));
        assert!(source_needs_password(ImportSource::OnePasswordOpvault));
        assert!(source_needs_password(ImportSource::Bitwarden));
        assert!(!source_needs_password(ImportSource::Csv));
        assert!(!source_needs_password(ImportSource::OpenKeyringBackup));
    }

    #[test]
    fn source_display_names() {
        assert_eq!(source_display(ImportSource::KeePass), "KeePass (.kdbx)");
        assert_eq!(source_display(ImportSource::Csv), "CSV");
        assert_eq!(
            source_display(ImportSource::OpenKeyringBackup),
            "OpenKeyring Backup (.okb)"
        );
    }

    #[test]
    fn display_password_masks() {
        let displayed = ImportExportScreen::display_password("hello");
        assert_eq!(displayed, "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
    }

    #[test]
    fn strength_color_mapping() {
        assert_eq!(
            ImportExportScreen::strength_color(&StrengthLevel::VeryWeak),
            ERROR
        );
        assert_eq!(
            ImportExportScreen::strength_color(&StrengthLevel::Weak),
            ERROR
        );
        assert_eq!(
            ImportExportScreen::strength_color(&StrengthLevel::Fair),
            WARNING
        );
        assert_eq!(
            ImportExportScreen::strength_color(&StrengthLevel::Strong),
            PRIMARY
        );
        assert_eq!(
            ImportExportScreen::strength_color(&StrengthLevel::VeryStrong),
            SUCCESS
        );
    }

    #[test]
    fn csv_mapping_defaults() {
        let screen = ImportExportScreen::new();
        assert_eq!(screen.csv_mapping.name_column, "Title");
        assert_eq!(screen.csv_mapping.username_column, "Username");
        assert_eq!(screen.csv_mapping.password_column, "Password");
        assert_eq!(screen.csv_mapping.url_column, "URL");
        assert_eq!(screen.csv_mapping.notes_column, "Notes");
        assert!(screen.csv_mapping.tags_column.is_none());
        assert!(screen.csv_mapping.skip_header);
    }

    #[test]
    fn export_strength_updates() {
        let mut screen = ImportExportScreen::new();
        assert!(screen.export_password_strength.is_none());

        screen.export_password = "a".to_string();
        screen.update_export_strength();
        assert_eq!(
            screen.export_password_strength.as_ref().unwrap().level,
            StrengthLevel::VeryWeak
        );

        screen.export_password.clear();
        screen.update_export_strength();
        assert!(screen.export_password_strength.is_none());
    }

    #[test]
    fn import_focus_cycle() {
        let mut screen = ImportExportScreen::new();
        // Default: KeePass (needs password, not CSV)
        screen.selected_source_idx = 0;
        assert_eq!(screen.import_focus, ImportFocus::SourceList);

        screen.import_focus_cycle_next();
        assert_eq!(screen.import_focus, ImportFocus::FilePath);

        screen.import_focus_cycle_next();
        assert_eq!(screen.import_focus, ImportFocus::Password);

        screen.import_focus_cycle_next();
        assert_eq!(screen.import_focus, ImportFocus::SourceList);

        // CSV: has csv fields
        screen.selected_source_idx = 4; // CSV
        screen.import_focus = ImportFocus::SourceList;
        screen.import_focus_cycle_next();
        assert_eq!(screen.import_focus, ImportFocus::FilePath);

        screen.import_focus_cycle_next();
        assert_eq!(screen.import_focus, ImportFocus::CsvName);

        screen.import_focus_cycle_next();
        assert_eq!(screen.import_focus, ImportFocus::CsvUsername);
    }

    #[test]
    fn export_focus_cycle() {
        let mut screen = ImportExportScreen::new();
        assert_eq!(screen.export_focus, ExportFocus::Scope);

        screen.export_focus = ExportFocus::ExportPassword;
        screen.export_focus = ExportFocus::ConfirmPassword;
        screen.export_focus = ExportFocus::OutputPath;
        screen.export_focus = ExportFocus::Scope;
    }

    #[test]
    fn export_scope_options() {
        let screen = ImportExportScreen::new();
        assert_eq!(screen.export_scope_option, ExportScopeOption::All);
    }
}
